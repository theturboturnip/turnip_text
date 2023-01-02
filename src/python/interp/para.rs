use anyhow::bail;
use pyo3::{prelude::*, types::PyDict};

use crate::{lexer::TTToken, parser::ParseSpan, python::{interop::*, interp::{InterpError, compute_action_for_code_mode}, typeclass::PyTcRef}};

use super::{InlineNodeToCreate, InterpBlockAction, InterpSpecialAction};

#[derive(Debug)]
pub(crate) struct InterpParaState {
    inl_state: InterpInlineState,
    para: Py<Paragraph>,
    sentence: Py<Sentence>,
    inline_stack: Vec<InterpInlineScopeState>,
}

#[derive(Debug)]
struct InterpInlineScopeState {
    scope: Py<InlineScope>,
    scope_start: ParseSpan,
    expected_n_hashes: usize,
}

/// Interpreter state specific to parsing paragraphs and the content within (i.e. inline content)
#[derive(Debug)]
enum InterpInlineState {
    /// When at the start of a line, ready for any inline token
    LineStart,
    /// When in the middle of a line, ready for any inline token
    MidLine,
    /// When in code mode
    BuildingCode {
        code: String,
        code_start: ParseSpan,
        expected_n_hashes: usize,
    },
    /// Having constructed some code which expects inline scope, expecting the next token to be an inline scope
    AttachingInlineLevelCode {
        owner: PyTcRef<InlineScopeOwner>,
        code_span: ParseSpan,
    },
    /// When building raw text
    BuildingRawText {
        text: String,
        raw_start: ParseSpan,
        expected_n_hashes: usize,
    },
}

#[derive(Debug)]
pub(crate) enum InterpParaAction {
    /// On encountering inline content within a paragraph, add it to the paragraph (starting a new one if necessary).
    ///
    /// - [InterpInlineState::LineStart] -> [InterpInlineState::MidLine]
    /// - [InterpInlineState::MidLine] -> [InterpInlineState::MidLine]
    /// - [InterpInlineState::BuildingCode] -> [InterpInlineState::MidLine]
    /// - [InterpInlineState::BuildingRawText] -> [InterpInlineState::MidLine]
    PushInlineContent(InlineNodeToCreate),

    /// Break the current sentence within the paragraph
    ///
    /// - [InterpInlineState::MidLine] -> [InterpInlineState::LineStart]
    BreakSentence,

    /// On encountering the start of an inline scope (i.e. an InlineScopeOpen optionally preceded by Python scope owner),
    /// push an inline scope onto existing paragraph state (or create a new one)
    ///
    /// - [InterpInlineState::LineStart] -> [InterpInlineState::MidLine]
    /// - [InterpInlineState::MidLine] -> [InterpInlineState::MidLine]
    /// - [InterpInlineState::AttachingInlineLevelCode] -> [InterpInlineState::MidLine]
    PushInlineScope(Option<PyTcRef<InlineScopeOwner>>, ParseSpan, usize),

    /// On encountering a scope close, pop the current inline scope off the stack
    /// (throwing an error if the stack is empty)
    /// - [InterpInlineState::LineStart] -> [InterpInlineState::MidLine]
    /// - [InterpInlineState::MidLine] -> [InterpInlineState::MidLine]
    PopInlineScope(ParseSpan),

    /// On encountering code within a paragraph, end the current inline token and enter code mode.
    ///
    /// - [InterpInlineState::LineStart] -> [InterpInlineState::BuildingCode]
    /// - [InterpInlineState::MidLine] -> [InterpInlineState::BuildingCode]
    StartInlineLevelCode(ParseSpan, usize),

    /// Having finished a code close which evals to [InlineScopeOwner],
    /// start a one-token wait for an inline scope to attach it to
    ///
    /// - [InterpInlineState::BuildingCode] -> [InterpInlineState::AttachingInlineLevelCode]
    /// - (other block state) -> [InterpInlineState::AttachingInlineLevelCode]
    WaitToAttachInlineCode(PyTcRef<InlineScopeOwner>, ParseSpan),

    /// See [InterpBlockAction::EndParagraph]
    ///
    /// Finish the paragraph and current sentence (raising an error if processing inline scopes)
    ///
    /// - [InterpInlineState::LineStart] -> (other block state)
    EndParagraph(ParseSpan),

    /// - [InterpInlineState::LineStart], [InterpInlineState::MidLine] -> (comment mode) + [InterpInlineState::MidLine]
    ///
    /// TODO should this break the current sentence or no?
    StartComment(ParseSpan),

    /// On encountering a raw scope open, start processing a raw block of text.
    ///
    /// - [InterpInlineState::LineStart] -> [InterpInlineState::BuildingRawText]
    /// - [InterpInlineState::MidLine] -> [InterpInlineState::BuildingRawText]
    /// - (other block state) -> [InterpInlineState::BuildingRawText]
    StartRawScope(ParseSpan, usize),
}

impl InterpParaState {
    pub(crate) fn new(py: Python) -> PyResult<Self> {
        Ok(Self {
            inl_state: InterpInlineState::LineStart,
            para: Py::new(py, Paragraph::new(py))?,
            sentence: Py::new(py, Sentence::new(py))?,
            inline_stack: vec![],
        })
    }

    pub(crate) fn para(&self) -> &Py<Paragraph> {
        &self.para
    }

    pub(crate) fn handle_token(
        &mut self,
        py: Python,
        globals: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> anyhow::Result<(Option<InterpBlockAction>, Option<InterpSpecialAction>)> {
        let actions = self.compute_action(py, globals, tok, data)?;
        self.handle_action(py, actions)
    }
    pub(crate) fn handle_action(
        &mut self,
        py: Python,
        action: Option<InterpParaAction>,
    ) -> anyhow::Result<(Option<InterpBlockAction>, Option<InterpSpecialAction>)> {
        if let Some(action) = action {
            use InterpInlineState as S;
            use InterpParaAction as A;
            let (new_inl_state, actions) = match (&self.inl_state, action) {
                (S::LineStart | S::MidLine, A::StartComment(span)) => (
                    S::MidLine,
                    (None, Some(InterpSpecialAction::StartComment(span))),
                ),

                (
                    S::LineStart | S::MidLine | S::BuildingCode { .. } | S::BuildingRawText { .. },
                    A::PushInlineContent(content),
                ) => {
                    let content = content.to_py(py)?;
                    self.push_to_topmost_scope(py, content.as_ref(py))?;
                    (S::MidLine, (None, None))
                }
                (S::MidLine, A::BreakSentence) => {
                    // If the sentence has stuff in it, push it into the paragraph and make a new one
                    if self.sentence.borrow(py).__len__(py) > 0 {
                        self.para.borrow_mut(py).push_sentence(self.sentence.as_ref(py))?;
                        self.sentence = Py::new(py, Sentence::new(py))?;
                    }
                    (S::LineStart, (None, None))
                }

                (
                    S::LineStart | S::MidLine | S::AttachingInlineLevelCode { .. },
                    A::PushInlineScope(owner, span, n),
                ) => {
                    let scope = InterpInlineScopeState {
                        scope: Py::new(py, InlineScope::new_rs(py, owner))?,
                        scope_start: span,
                        expected_n_hashes: n,
                    };
                    self.inline_stack.push(scope);
                    (S::MidLine, (None, None))
                }
                (
                    S::LineStart | S::MidLine,
                    A::PopInlineScope(scope_close_span)
                ) => {
                    let popped_scope = self.inline_stack.pop();
                    match popped_scope {
                        Some(popped_scope) => self.push_to_topmost_scope(py, popped_scope.scope.as_ref(py))?,
                        // TODO should specify *inline* scope, not all scopes
                        None => bail!(InterpError::ScopeCloseOutsideScope(scope_close_span))
                    };
                    (S::MidLine, (None, None))
                }

                (
                    S::LineStart | S::MidLine, // or another block state, which would be inited as InitState
                    A::StartRawScope(raw_start, expected_n_hashes),
                ) => (
                    S::BuildingRawText {
                        text: "".into(),
                        raw_start,
                        expected_n_hashes,
                    },
                    (None, None),
                ),

                (
                    S::LineStart | S::MidLine,
                    A::StartInlineLevelCode(code_start, expected_n_hashes),
                ) => (
                    S::BuildingCode {
                        code: "".into(),
                        code_start,
                        expected_n_hashes,
                    },
                    (None, None),
                ),
                (
                    S::LineStart | S::BuildingCode { .. },
                    A::WaitToAttachInlineCode(owner, code_span),
                ) => (
                    S::AttachingInlineLevelCode { owner, code_span },
                    (None, None),
                ),

                (S::LineStart, A::EndParagraph(end_para_span)) => {
                    if let Some(i) = self.inline_stack.last() {
                        bail!(InterpError::ParaBreakInInlineScope {
                            scope_start: i.scope_start,
                            para_break: end_para_span
                        })
                    }
                    // If the sentence has stuff in it, push it into the paragraph and make a new one
                    if self.sentence.borrow(py).__len__(py) > 0 {
                        self.para.borrow_mut(py).push_sentence(self.sentence.as_ref(py))?;
                        self.sentence = Py::new(py, Sentence::new(py))?;
                    }
                    (S::LineStart, (Some(InterpBlockAction::EndParagraph), None))
                }

                (_, action) => bail!(
                    "Invalid inline state/action pair encountered ({0:?}, {1:?})",
                    self.inl_state,
                    action
                ),
            };
            self.inl_state = new_inl_state;
            Ok(actions)
        } else {
            Ok((None, None))
        }
    }
    fn compute_action(
        &mut self,
        py: Python,
        globals: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> anyhow::Result<Option<InterpParaAction>> {
        use InterpParaAction::*;
        use TTToken::*;

        let action = match &mut self.inl_state {
            InterpInlineState::LineStart => match tok {
                Newline(span) => Some(EndParagraph(span)),
                Hashes(span, _) => Some(StartComment(span)),

                CodeOpen(span, n) => Some(StartInlineLevelCode(span, n)),
                BlockScopeOpen(span, _) => {
                    bail!(InterpError::BlockScopeOpenedMidPara { scope_start: span })
                }
                InlineScopeOpen(span, n) => Some(PushInlineScope(None, span, n)),
                RawScopeOpen(span, n) => Some(StartRawScope(span, n)),

                CodeClose(span, _) => bail!(InterpError::CodeCloseOutsideCode(span)),
                ScopeClose(span, n_hashes) => match self.inline_stack.last() {
                    Some(InterpInlineScopeState {
                        expected_n_hashes,
                        ..
                    }) if n_hashes == *expected_n_hashes => Some(PopInlineScope(span)),
                    Some(InterpInlineScopeState {
                        expected_n_hashes,
                        scope_start,
                        ..
                    }) => Err(InterpError::MismatchingScopeClose {
                        n_hashes,
                        expected_n_hashes: *expected_n_hashes,
                        scope_open_span: *scope_start,
                        scope_close_span: span,
                    })?,
                    None => Err(InterpError::ScopeCloseOutsideScope(span))?,
                },

                _ => Some(PushInlineContent(InlineNodeToCreate::UnescapedText(
                    tok.stringify(data).into(),
                ))),
            },
            InterpInlineState::MidLine => match tok {
                // Newline => Sentence break (TODO this needs to be changed, we at least need to be able to escape it?)
                Newline(_) => Some(BreakSentence),
                Hashes(span, _) => Some(StartComment(span)),

                CodeOpen(span, n) => Some(StartInlineLevelCode(span, n)),
                BlockScopeOpen(span, _) => {
                    bail!(InterpError::BlockScopeOpenedMidPara { scope_start: span })
                }
                InlineScopeOpen(span, n) => Some(PushInlineScope(None, span, n)),
                RawScopeOpen(span, n) => Some(StartRawScope(span, n)),

                CodeClose(span, _) => bail!(InterpError::CodeCloseOutsideCode(span)),
                ScopeClose(span, n_hashes) => match self.inline_stack.last() {
                    Some(InterpInlineScopeState {
                        expected_n_hashes,
                        ..
                    }) if n_hashes == *expected_n_hashes => Some(PopInlineScope(span)),
                    Some(InterpInlineScopeState {
                        expected_n_hashes,
                        scope_start,
                        ..
                    }) => Err(InterpError::MismatchingScopeClose {
                        n_hashes,
                        expected_n_hashes: *expected_n_hashes,
                        scope_open_span: *scope_start,
                        scope_close_span: span,
                    })?,
                    None => Err(InterpError::ScopeCloseOutsideScope(span))?,
                },

                _ => Some(PushInlineContent(InlineNodeToCreate::UnescapedText(
                    tok.stringify(data).into(),
                ))),
            },
            InterpInlineState::BuildingCode {
                code,
                code_start,
                expected_n_hashes,
            } => {
                let code_span =
                    compute_action_for_code_mode(data, tok, code, code_start, *expected_n_hashes);
                match code_span {
                    Some(code_span) => {
                        // The code ended...
                        // TODO PyErr -> InterpErr?
                        let res = EvalBracketResult::eval(py, globals, code.as_str())?;
                        let inl_action = match res {
                            EvalBracketResult::Block(_) => bail!(InterpError::BlockOwnerCodeMidPara { code_span }),
                            EvalBracketResult::Inline(i) => WaitToAttachInlineCode(i, code_span),
                            EvalBracketResult::Other(s) => PushInlineContent(InlineNodeToCreate::UnescapedPyString(s)),
                        };
                        Some(inl_action)
                        // if eval_result is a BlockScopeOwner, fail! can't have block scope inside inline text
                        // elif eval_result is an InlineScopeOwner, WaitToAttachInlineCode
                        // else stringify and PushInlineContent
                    }
                    None => None,
                }
            }
            InterpInlineState::AttachingInlineLevelCode { owner, code_span } => match tok {
                InlineScopeOpen(span, n_hashes) => {
                    Some(PushInlineScope(Some(owner.clone()), span, n_hashes))
                }
                _ => bail!(InterpError::InlineOwnerCodeHasNoScope {
                    code_span: *code_span
                }),
            },
            InterpInlineState::BuildingRawText {
                text,
                expected_n_hashes,
                ..
            } => match tok {
                ScopeClose(_, n) if n == *expected_n_hashes => {
                    Some(PushInlineContent(InlineNodeToCreate::RawText(text.clone())))
                }
                _ => {
                    text.push_str(tok.stringify(data));
                    None
                }
            },
        };
        Ok(action)
    }

    fn push_to_topmost_scope(&self, py: Python, node: &PyAny) -> PyResult<()> {
        match self.inline_stack.last() {
            Some(i) => i.scope.borrow_mut(py).push_node(node),
            None => self.sentence.borrow_mut(py).push_node(node),
        }
    }
}