use pyo3::{prelude::*, types::PyDict};

use crate::{
    lexer::{Escapable, TTToken},
    python::{
        interop::*,
        interp::{handle_code_mode, InterpError, MapInterpResult},
        typeclass::PyTcRef,
    },
    util::ParseSpan,
};

use super::{InlineNodeToCreate, InterpBlockTransition, InterpResult, InterpSpecialTransition};

#[derive(Debug)]
pub(crate) struct InterpParaState {
    para: Py<Paragraph>,
    sentence: Py<Sentence>,
    inline_stack: Vec<InterpInlineScopeState>,
    sentence_state: InterpSentenceState,
}

#[derive(Debug)]
struct InterpInlineScopeState {
    scope: Py<InlineScope>,
    scope_start: ParseSpan,
}

/// Interpreter state specific to parsing paragraphs and the content within (i.e. inline content)
#[derive(Debug)]
enum InterpSentenceState {
    /// When at the start of a sentence, ready for any inline token
    SentenceStart,
    /// When in the middle of a sentence, ready for any inline token
    MidSentence,
    /// After encountering text, allow further text to be merged in
    BuildingText(String),
    /// When in code mode
    BuildingCode {
        code: String,
        code_start: ParseSpan,
        expected_n_hashes: usize,
    },
    /// Having constructed some code which expects inline scope,
    /// expecting the next token to be an inline or raw scope
    AttachingInlineLevelCode {
        owner: PyTcRef<InlineScopeOwner>,
        code_span: ParseSpan,
    },
    /// When building raw text, optionally attached to an InlineScopeOwner
    BuildingRawText {
        owner: Option<PyTcRef<InlineScopeOwner>>,
        text: String,
        raw_start: ParseSpan,
        expected_n_hashes: usize,
    },
}

#[derive(Debug)]
pub(crate) enum InterpParaTransition {
    /// On encountering inline content within a paragraph, add it to the paragraph (starting a new one if necessary).
    ///
    /// - [InterpSentenceState::SentenceStart] -> [InterpSentenceState::MidSentence]
    /// - [InterpSentenceState::MidSentence] -> [InterpSentenceState::MidSentence]
    /// - [InterpSentenceState::BuildingCode] -> [InterpSentenceState::MidSentence]
    /// - [InterpSentenceState::BuildingRawText] -> [InterpSentenceState::MidSentence]
    PushInlineContent(InlineNodeToCreate),

    /// Break the current sentence within the paragraph.
    /// Finishes the current BuildingText if in progress, and pushes it to the topmost scope (which should be the sentence)
    /// Errors out if inline scopes are currently open - right now inline scopes must be entirely within a sentence.
    ///
    /// - [InterpSentenceState::MidSentence] -> [InterpSentenceState::SentenceStart]
    BreakSentence,

    /// On encountering the start of an inline scope (i.e. an InlineScopeOpen optionally preceded by Python scope owner),
    /// push an inline scope onto existing paragraph state (or create a new one).
    ///
    /// Finishes the current BuildingText if in progress, pushes it to topmost scope before creating new scope.
    ///
    /// - [InterpSentenceState::SentenceStart] -> [InterpSentenceState::MidSentence]
    /// - [InterpSentenceState::MidSentence] -> [InterpSentenceState::MidSentence]
    /// - [InterpSentenceState::AttachingInlineLevelCode] -> [InterpSentenceState::MidSentence]
    /// - [InterpSentenceState::BuildingText] -> [InterpSentenceState::MidSentence]
    PushInlineScope(Option<PyTcRef<InlineScopeOwner>>, ParseSpan),

    /// On encountering a scope close, pop the current inline scope off the stack
    /// (pushing the current BuildingText to that scope beforehand)
    /// (throwing an error if the stack is empty)
    /// - [InterpSentenceState::SentenceStart] -> [InterpSentenceState::MidSentence]
    /// - [InterpSentenceState::MidSentence] -> [InterpSentenceState::MidSentence]
    /// - [InterpSentenceState::BuildingText] -> [InterpSentenceState::MidSentence]
    PopInlineScope(ParseSpan),

    /// On encountering code within a paragraph, end the current inline token and enter code mode.
    /// (pushing the current BuildingText to that scope beforehand)
    ///
    /// - [InterpSentenceState::SentenceStart] -> [InterpSentenceState::BuildingCode]
    /// - [InterpSentenceState::MidSentence] -> [InterpSentenceState::BuildingCode]
    /// - [InterpSentenceState::BuildingText] -> [InterpSentenceState::BuildingCode]
    StartInlineLevelCode(ParseSpan, usize),

    /// Having finished a code close which evals to [InlineScopeOwner],
    /// start a one-token wait for an inline or raw scope to attach it to
    ///
    /// - [InterpSentenceState::BuildingCode] -> [InterpSentenceState::AttachingInlineLevelCode]
    /// - (other block state) -> [InterpSentenceState::AttachingInlineLevelCode]
    WaitToAttachInlineCode(PyTcRef<InlineScopeOwner>, ParseSpan),

    /// See [InterpBlockTransition::EndParagraph]
    ///
    /// Finish the paragraph and current sentence (raising an error if processing inline scopes)
    ///
    /// Contains None if request was brought up by EOF
    ///
    /// - [InterpSentenceState::SentenceStart] -> (other block state)
    EndParagraph(Option<ParseSpan>),

    /// See [InterpBlockTransition::EndParagraphAndPopBlock]
    ///
    /// Finish the paragraph and current sentence (raising an error if processing inline scopes),
    /// and pop the block
    ///
    /// - [InterpSentenceState::SentenceStart] -> (other block state)
    EndParagraphAndPopBlock(ParseSpan),

    /// Finishes the current BuildingText if in progress, pushes it to topmost scope, enters comment mode
    ///
    /// TODO should this break the current sentence or no?
    ///
    /// - [InterpSentenceState::SentenceStart] -> (comment mode) + [InterpSentenceState::MidSentence]
    /// - [InterpSentenceState::MidSentence] -> (comment mode) + [InterpSentenceState::MidSentence]
    /// - [InterpSentenceState::BuildingText] -> (comment mode) + [InterpSentenceState::MidSentence]
    StartComment(ParseSpan),

    /// On encountering a raw scope open, start processing a raw block of text.
    /// Finishes the current BuildingText if in progress, pushes it to topmost scope.
    ///
    /// - [InterpSentenceState::SentenceStart] -> [InterpSentenceState::BuildingRawText]
    /// - [InterpSentenceState::MidSentence] -> [InterpSentenceState::BuildingRawText]
    /// - [InterpSentenceState::BuildingText] -> [InterpSentenceState::BuildingRawText]
    /// - [InterpSentenceState::AttachingInlineLevelCode] -> [InterpSentenceState::BuildingRawText]
    /// - (other block state) -> [InterpSentenceState::BuildingRawText]
    StartRawScope(Option<PyTcRef<InlineScopeOwner>>, ParseSpan, usize),

    /// On encountering inline text, start processing a string of text
    ///
    /// - [InterpSentenceState::SentenceStart] -> [InterpSentenceState::BuildingText]
    /// - [InterpSentenceState::MidSentence] -> [InterpSentenceState::BuildingText]
    /// - (other block state) -> [InterpSentenceState::BuildingText]
    StartText(String),
}

impl InterpParaState {
    pub(crate) fn new(py: Python) -> PyResult<Self> {
        Ok(Self {
            sentence_state: InterpSentenceState::SentenceStart,
            para: Py::new(py, Paragraph::new(py))?,
            sentence: Py::new(py, Sentence::new(py))?,
            inline_stack: vec![],
        })
    }

    pub(crate) fn para(&self) -> &Py<Paragraph> {
        &self.para
    }

    pub(crate) fn finalize(
        &mut self,
        py: Python,
    ) -> InterpResult<(
        Option<InterpBlockTransition>,
        Option<InterpSpecialTransition>,
    )> {
        match self.sentence_state {
            InterpSentenceState::SentenceStart | InterpSentenceState::MidSentence => {
                // This will automatically check if we're inside an inline scope
                self.handle_transition(py, Some(InterpParaTransition::EndParagraph(None)))
            }
            InterpSentenceState::BuildingText(_) => {
                self.handle_transition(py, Some(InterpParaTransition::BreakSentence))?;
                // This will automatically check if we're inside an inline scope
                self.handle_transition(py, Some(InterpParaTransition::EndParagraph(None)))
            }
            // Error states
            InterpSentenceState::BuildingCode { code_start, .. } => {
                return Err(InterpError::EndedInsideCode { code_start })
            }
            InterpSentenceState::AttachingInlineLevelCode { code_span, .. } => {
                return Err(InterpError::InlineOwnerCodeHasNoScope { code_span })
            }
            InterpSentenceState::BuildingRawText { raw_start, .. } => {
                return Err(InterpError::EndedInsideRawScope {
                    raw_scope_start: raw_start,
                })
            }
        }
    }

    pub(crate) fn handle_token(
        &mut self,
        py: Python,
        globals: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> InterpResult<(
        Option<InterpBlockTransition>,
        Option<InterpSpecialTransition>,
    )> {
        let transition = self.mutate_state_and_find_transition(py, globals, tok, data)?;
        self.handle_transition(py, transition)
    }
    pub(crate) fn handle_transition(
        &mut self,
        py: Python,
        transition: Option<InterpParaTransition>,
    ) -> InterpResult<(
        Option<InterpBlockTransition>,
        Option<InterpSpecialTransition>,
    )> {
        if let Some(transition) = transition {
            use InterpParaTransition as T;
            use InterpSentenceState as S;

            // All transitions interrupt the current Text token
            if let S::BuildingText(text) = &self.sentence_state {
                // Finish the text-in-progress and push to topmost scope
                self.push_built_text_to_topmost_scope(py, text)?;
            }

            let (new_inl_state, transitions) = match (&self.sentence_state, transition) {
                (S::SentenceStart | S::MidSentence | S::BuildingText(_), T::StartComment(span)) => {
                    (
                        S::MidSentence,
                        (None, Some(InterpSpecialTransition::StartComment(span))),
                    )
                }

                (S::SentenceStart | S::MidSentence, T::StartText(text)) => {
                    (S::BuildingText(text), (None, None))
                }

                (
                    S::SentenceStart
                    | S::MidSentence
                    | S::BuildingCode { .. }
                    | S::BuildingRawText { .. }
                    | S::BuildingText(_),
                    T::PushInlineContent(content),
                ) => {
                    let content = content.to_py(py)?;
                    self.push_to_topmost_scope(py, content.as_ref(py))?;
                    (S::MidSentence, (None, None))
                }
                (S::MidSentence | S::BuildingText(_), T::BreakSentence) => {
                    // Ensure we don't have any inline scopes
                    self.check_inline_scopes_closed().map_err(|scope_start| {
                        InterpError::SentenceBreakInInlineScope { scope_start }
                    })?;
                    // If the sentence has stuff in it, push it into the paragraph and make a new one
                    if self.sentence.borrow(py).__len__(py) > 0 {
                        self.para
                            .borrow_mut(py)
                            .push_sentence(self.sentence.as_ref(py))
                            .err_as_interp_internal(py)?;
                        self.sentence =
                            Py::new(py, Sentence::new(py)).err_as_interp_internal(py)?;
                    }
                    (S::SentenceStart, (None, None))
                }

                (
                    S::SentenceStart
                    | S::MidSentence
                    | S::AttachingInlineLevelCode { .. }
                    | S::BuildingText(_),
                    T::PushInlineScope(owner, span),
                ) => {
                    let scope = InterpInlineScopeState {
                        scope: Py::new(py, InlineScope::new_rs(py, owner))
                            .err_as_interp_internal(py)?,
                        scope_start: span,
                    };
                    self.inline_stack.push(scope);
                    (S::MidSentence, (None, None))
                }
                (S::SentenceStart | S::MidSentence | S::BuildingText(_), T::PopInlineScope(_)) => {
                    let popped_scope = self.inline_stack.pop();
                    match popped_scope {
                        Some(popped_scope) => self.push_to_topmost_scope(py, popped_scope.scope.as_ref(py))?,
                        None => {
                            return Err(InterpError::InternalErr("PopInlineScope attempted with no inline scopes - should use EndParagraphAndPopBlock in this case".into()))
                        }
                    };
                    (S::MidSentence, (None, None))
                }

                (
                    S::SentenceStart
                    | S::MidSentence
                    | S::BuildingText(_)
                    | S::AttachingInlineLevelCode { .. }, // or another block state, which would be inited as InitState
                    T::StartRawScope(owner, raw_start, expected_n_hashes),
                ) => (
                    S::BuildingRawText {
                        owner,
                        text: "".into(),
                        raw_start,
                        expected_n_hashes,
                    },
                    (None, None),
                ),

                (
                    S::SentenceStart | S::MidSentence | S::BuildingText(_),
                    T::StartInlineLevelCode(code_start, expected_n_hashes),
                ) => (
                    S::BuildingCode {
                        code: "".into(),
                        code_start,
                        expected_n_hashes,
                    },
                    (None, None),
                ),
                (
                    S::SentenceStart | S::BuildingCode { .. },
                    T::WaitToAttachInlineCode(owner, code_span),
                ) => (
                    S::AttachingInlineLevelCode { owner, code_span },
                    (None, None),
                ),

                (
                    S::SentenceStart | S::MidSentence | S::BuildingText(_),
                    T::EndParagraph(para_break),
                ) => {
                    if let Err(scope_start) = self.check_inline_scopes_closed() {
                        if let Some(para_break) = para_break {
                            return Err(InterpError::ParaBreakInInlineScope {
                                scope_start,
                                para_break,
                            });
                        } else {
                            return Err(InterpError::EndedInsideScope { scope_start });
                        }
                    }
                    // If the sentence has stuff in it, push it into the paragraph and make a new one
                    if self.sentence.borrow(py).__len__(py) > 0 {
                        self.para
                            .borrow_mut(py)
                            .push_sentence(self.sentence.as_ref(py))
                            .err_as_interp_internal(py)?;
                        self.sentence =
                            Py::new(py, Sentence::new(py)).err_as_interp_internal(py)?;
                    }
                    (
                        S::SentenceStart,
                        (Some(InterpBlockTransition::EndParagraph), None),
                    )
                }

                (
                    S::SentenceStart | S::MidSentence | S::BuildingText(_),
                    T::EndParagraphAndPopBlock(scope_end_span),
                ) => {
                    // This is only called when all inline scopes are closed - just assert they are
                    self.check_inline_scopes_closed().map_err(|_| {
                        InterpError::InternalErr("paragraph EndParagraphAndPopBlock transition invoked when inline scopes are still on the stack".into())
                    })?;
                    // If the sentence has stuff in it, push it into the paragraph and make a new one
                    if self.sentence.borrow(py).__len__(py) > 0 {
                        self.para
                            .borrow_mut(py)
                            .push_sentence(self.sentence.as_ref(py))
                            .err_as_interp_internal(py)?;
                        self.sentence =
                            Py::new(py, Sentence::new(py)).err_as_interp_internal(py)?;
                    }
                    (
                        S::SentenceStart,
                        (
                            Some(InterpBlockTransition::EndParagraphAndPopBlock(
                                scope_end_span,
                            )),
                            None,
                        ),
                    )
                }

                (_, transition) => {
                    return Err(InterpError::InternalErr(format!(
                        "Invalid inline state/transition pair encountered ({0:?}, {1:?})",
                        self.sentence_state, transition
                    )))
                }
            };
            self.sentence_state = new_inl_state;
            Ok(transitions)
        } else {
            Ok((None, None))
        }
    }
    fn mutate_state_and_find_transition(
        &mut self,
        py: Python,
        globals: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> InterpResult<Option<InterpParaTransition>> {
        use InterpParaTransition::*;
        use TTToken::*;

        let transition = match &mut self.sentence_state {
            InterpSentenceState::SentenceStart => match tok {
                // Escaped newline => "Continue sentence".
                // at the start of a sentence, "Continue sentence" has no meaning
                Escaped(_, Escapable::Newline) => None,

                Newline(span) => Some(EndParagraph(Some(span))),
                Hashes(span, _) => Some(StartComment(span)),

                CodeOpen(span, n) => Some(StartInlineLevelCode(span, n)),
                BlockScopeOpen(span) => {
                    return Err(InterpError::BlockScopeOpenedMidPara { scope_start: span })
                }
                InlineScopeOpen(span) => Some(PushInlineScope(None, span)),
                RawScopeOpen(span, n) => Some(StartRawScope(None, span, n)),

                CodeClose(span, _) => return Err(InterpError::CodeCloseOutsideCode(span)),
                ScopeClose(span) => Some(self.try_pop_scope(py, span)?),
                RawScopeClose(span, _) => {
                    return Err(InterpError::RawScopeCloseOutsideRawScope(span))
                }

                _ => Some(StartText(tok.stringify_escaped(data).into())),
            },
            InterpSentenceState::MidSentence => match tok {
                // Escaped newline => "Continue sentence" i.e. no sentence break
                // mid-sentence, "Continue sentence" just means "do nothing"
                Escaped(_, Escapable::Newline) => None,

                // Newline => Sentence break
                Newline(_) => Some(BreakSentence),
                Hashes(span, _) => Some(StartComment(span)),

                CodeOpen(span, n) => Some(StartInlineLevelCode(span, n)),
                BlockScopeOpen(span) => {
                    return Err(InterpError::BlockScopeOpenedMidPara { scope_start: span })
                }
                InlineScopeOpen(span) => Some(PushInlineScope(None, span)),
                RawScopeOpen(span, n) => Some(StartRawScope(None, span, n)),

                CodeClose(span, _) => return Err(InterpError::CodeCloseOutsideCode(span)),
                ScopeClose(span) => Some(self.try_pop_scope(py, span)?),
                RawScopeClose(span, _) => {
                    return Err(InterpError::RawScopeCloseOutsideRawScope(span))
                }

                _ => Some(StartText(tok.stringify_escaped(data).into())),
            },
            InterpSentenceState::BuildingText(text) => match tok {
                // Escaped newline => "Continue sentence".
                // mid-sentence, "Continue sentence" has no meaning
                Escaped(_, Escapable::Newline) => None,

                // Newline => Sentence break (TODO this needs to be changed, we at least need to be able to escape it?)
                Newline(_) => Some(BreakSentence),
                Hashes(span, _) => Some(StartComment(span)),

                CodeOpen(span, n) => Some(StartInlineLevelCode(span, n)),
                BlockScopeOpen(span) => {
                    return Err(InterpError::BlockScopeOpenedMidPara { scope_start: span })
                }
                InlineScopeOpen(span) => Some(PushInlineScope(None, span)),
                RawScopeOpen(span, n) => Some(StartRawScope(None, span, n)),

                CodeClose(span, _) => return Err(InterpError::CodeCloseOutsideCode(span)),
                ScopeClose(span) => Some(self.try_pop_scope(py, span)?),
                RawScopeClose(span, _) => {
                    return Err(InterpError::RawScopeCloseOutsideRawScope(span))
                }

                _ => {
                    text.push_str(tok.stringify_escaped(data));
                    None
                }
            },
            InterpSentenceState::BuildingCode {
                code,
                code_start,
                expected_n_hashes,
            } => {
                let code_span = handle_code_mode(data, tok, code, code_start, *expected_n_hashes);
                match code_span {
                    Some(code_span) => {
                        // The code ended...
                        use EvalBracketResult::*;

                        // The code ended...
                        let res = EvalBracketResult::eval(py, globals, code.as_str())
                            .err_as_interp(py, code_span)?;
                        let inl_transition = match res {
                            Block(_) => {
                                return Err(InterpError::BlockOwnerCodeMidPara { code_span })
                            }
                            Inline(i) => WaitToAttachInlineCode(i, code_span),
                            Other(s) => PushInlineContent(InlineNodeToCreate::UnescapedPyString(s)),
                        };
                        Some(inl_transition)
                        // if eval_result is a BlockScopeOwner, fail! can't have block scope inside inline text
                        // elif eval_result is an InlineScopeOwner, WaitToAttachInlineCode
                        // else stringify and PushInlineContent
                    }
                    None => None,
                }
            }
            InterpSentenceState::AttachingInlineLevelCode { owner, code_span } => match tok {
                InlineScopeOpen(span) => Some(PushInlineScope(Some(owner.clone()), span)),
                RawScopeOpen(span, n_hashes) => {
                    Some(StartRawScope(Some(owner.clone()), span, n_hashes))
                }
                _ => {
                    return Err(InterpError::InlineOwnerCodeHasNoScope {
                        code_span: *code_span,
                    })
                }
            },
            InterpSentenceState::BuildingRawText {
                owner,
                text,
                expected_n_hashes,
                ..
            } => match tok {
                RawScopeClose(_, n_hashes) if n_hashes == *expected_n_hashes => Some(
                    PushInlineContent(InlineNodeToCreate::RawText(owner.clone(), text.clone())),
                ),
                _ => {
                    text.push_str(tok.stringify_raw(data));
                    None
                }
            },
        };
        Ok(transition)
    }

    fn try_pop_scope(
        &mut self,
        py: Python,
        scope_close_span: ParseSpan,
    ) -> InterpResult<InterpParaTransition> {
        match self.inline_stack.last() {
            Some(InterpInlineScopeState { .. }) => {
                Ok(InterpParaTransition::PopInlineScope(scope_close_span))
            }
            None => {
                // If the sentence has stuff in it, push it into the paragraph and make a new one
                if self.sentence.borrow(py).__len__(py) > 0 {
                    self.para
                        .borrow_mut(py)
                        .push_sentence(self.sentence.as_ref(py))
                        .err_as_interp_internal(py)?;
                    self.sentence = Py::new(py, Sentence::new(py)).err_as_interp_internal(py)?;
                }
                Ok(InterpParaTransition::EndParagraphAndPopBlock(
                    scope_close_span,
                ))
            }
        }
    }

    /// Check if all inline scopes are closed, returning [Err] of [ParseSpan] of the closest open inline scope if not.
    fn check_inline_scopes_closed(&self) -> Result<(), ParseSpan> {
        if let Some(i) = self.inline_stack.last() {
            Err(i.scope_start)
        } else {
            Ok(())
        }
    }

    fn push_to_topmost_scope(&self, py: Python, node: &PyAny) -> InterpResult<()> {
        match self.inline_stack.last() {
            Some(i) => i.scope.borrow_mut(py).push_node(node),
            None => self.sentence.borrow_mut(py).push_node(node),
        }
        .err_as_interp_internal(py)
    }

    fn push_built_text_to_topmost_scope(&self, py: Python, text: &String) -> InterpResult<()> {
        let node = InlineNodeToCreate::UnescapedText(text.clone()).to_py(py)?;
        self.push_to_topmost_scope(py, node.as_ref(py))
    }
}
