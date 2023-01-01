#![allow(dead_code)]

use std::ffi::CStr;

use anyhow::bail;
use pyembed::{ExtensionModule, MainPythonInterpreter, OxidizedPythonInterpreterConfig};
use pyo3::{prelude::*, types::PyDict};
use thiserror::Error;

mod interop;
use interop::*;

use crate::{lexer::TTToken, parser::ParseSpan};

/// Struct holding references to current Python state, including the relevant globals/locals.
pub struct TurnipTextPython<'interp> {
    pub interp: MainPythonInterpreter<'interp, 'interp>,
    pub globals: Py<PyDict>,
}

fn interpreter_config<'a>() -> OxidizedPythonInterpreterConfig<'a> {
    let mut base_config = OxidizedPythonInterpreterConfig::default();
    // Clear argv - our command-line arguments are not useful for the embedded python
    base_config.argv = Some(vec![]);
    base_config.extra_extension_modules = Some(vec![ExtensionModule {
        name: CStr::from_bytes_with_nul(turnip_text::NAME.as_bytes())
            .unwrap()
            .to_owned(),
        init_func: turnip_text::init,
    }]);
    // "If this is false, the default path configuration built into libpython is used."
    // This avoids a `init_fs_encoding` error message, where python tries to import the standard library and fails because we've told it the stdlib is installed relative to the executable
    base_config.set_missing_path_configuration = false;
    base_config
}

impl<'interp> TurnipTextPython<'interp> {
    pub fn new() -> TurnipTextPython<'interp> {
        let interp = MainPythonInterpreter::new(interpreter_config())
            .expect("Couldn't create python interpreter");

        pyo3::prepare_freethreaded_python();
        let globals = interp
            .with_gil(|py| -> PyResult<Py<PyDict>> {
                let globals = PyDict::new(py);
                py.run("from turnip_text import *", Some(globals), None)?;
                Ok(globals.into())
            })
            .unwrap();

        Self { interp, globals }
    }

    pub fn with_gil<F, R>(&self, f: F) -> R
    where
        F: for<'py> FnOnce(Python<'py>, &'py PyDict) -> R,
    {
        self.interp
            .with_gil(|py| -> R { f(py, self.globals.as_ref(py)) })
    }
}

/// Block-level state for the interpreter
#[derive(Debug)]
enum InterpBlockState {
    /// Waiting for new content to transition into [Self::WritingPara] or [Self::BuildingBlockLevelCode]
    ReadyForNewBlock,
    /// Building a paragraph block node, which will be added to the parent block node once complete.
    ///
    /// Transitions to [Self::ReadyForNewBlock] after finishing the paragraph
    WritingPara(InterpParaState),
    /// Building Python-code to evaluate at the block level, outside a paragraph
    ///
    /// Transitions to [Self::AttachingBlockLevelCode] once finished
    BuildingBlockLevelCode {
        code: String,
        code_start: ParseSpan,
        expected_n_hashes: usize,
    },
    /// Having constructed some code which evaluated to a BlockScopeOwner,
    /// assert the next token is a block scope and glue this code to it.
    ///
    /// Transitions to [Self::ReadyForNewBlock]
    AttachingBlockLevelCode {
        owner: Py<BlockScopeOwner>,
        code_span: ParseSpan,
    },
}
#[derive(Debug)]
struct InterpParaState {
    inl_state: InterpInlineState,
    para: Py<Paragraph>,
    sentence: Py<Sentence>,
    inline_stack: Vec<Py<InlineScope>>,
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
        owner: Py<InlineScopeOwner>,
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
struct InterpCommentState {
    comment_start: ParseSpan,
}

#[derive(Debug)]
struct InterpBlockScopeState {
    node: Py<BlockNode>,
    scope_start: ParseSpan,
    expected_n_hashes: usize,
}

struct InterpState<'interp, 'a: 'interp> {
    /// FSM state
    block_state: InterpBlockState,
    /// Overrides InterpBlockState and raw_state - if Some(state), we are in "comment mode" and all other state machines are paused
    comment_state: Option<InterpCommentState>,
    /// Stack of block scopes
    block_stack: Vec<InterpBlockScopeState>,
    /// Root of the document
    root: Py<BlockNode>,
    /// Python interpreter and context
    ttpython: &'a mut TurnipTextPython<'interp>,
    /// Raw text data
    data: &'a str,
}

#[derive(Debug)]
enum InterpBlockAction {
    /// On encountering a CodeOpen at the start of a line, start gathering block level code
    ///
    /// - [InterpBlockState::ReadyForNewBlock] -> [InterpBlockState::BuildingBlockLevelCode]
    /// - All others invalid
    StartBlockLevelCode(ParseSpan, usize),

    /// Having finished a block-level code close which evals to [BlockScopeOwner],
    /// start a one-token wait for an inline scope to attach it to
    ///
    /// - [InterpBlockState::BuildingBlockLevelCode] -> [InterpBlockState::AttachingBlockLevelCode]
    WaitToAttachBlockCode(Py<BlockScopeOwner>, ParseSpan),

    /// Start a paragraph, optionally executing an action on the paragraph-level state machine
    ///
    /// - [InterpBlockState::ReadyForNewBlock] -> [InterpBlockState::WritingPara]
    /// - [InterpBlockState::BuildingBlockLevelCode] -> [InterpBlockState::WritingPara]
    StartParagraph(Option<InterpParaAction>),

    /// On encountering a paragraph break (a blank line), add the paragraph to the document
    ///
    /// - [InterpBlockState::WritingPara] -> [InterpBlockState::ReadyForNewBlock]
    EndParagraph,

    /// On encountering block content (i.e. a BlockScopeOpen optionally preceded by a Python scope owner),
    /// push a block scope onto the current stack
    ///
    /// - [InterpBlockState::ReadyForNewBlock] -> [InterpBlockState::ReadyForNewBlock]
    /// - [InterpBlockState::AttachingBlockLevelCode] -> [InterpBlockState::ReadyForNewBlock]
    PushBlock(Option<Py<BlockScopeOwner>>, ParseSpan, usize),

    /// On encountering a scope close, pop the current block from the scope
    ///
    /// - [InterpBlockState::ReadyForNewBlock] -> [InterpBlockState::ReadyForNewBlock]
    PopBlock(ParseSpan),
}

#[derive(Debug)]
enum InterpSpecialAction {
    /// On encountering a comment starter, go into comment mode
    StartComment(ParseSpan),

    /// Leave comment mode
    EndComment,
}

#[derive(Debug)]
enum InterpParaAction {
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
    PushInlineScope(Option<Py<InlineScopeOwner>>, ParseSpan, usize),

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
    WaitToAttachInlineCode(Py<InlineScopeOwner>, ParseSpan),

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

#[derive(Debug)]
enum InlineNodeToCreate {
    UnescapedText(String),
    RawText(String),
}

/// Enumeration of all possible interpreter errors
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InterpError {
    #[error("Code close encountered outside of code mode")]
    CodeCloseOutsideCode(ParseSpan),
    #[error("Scope close encountered with no matching scope open")]
    ScopeCloseOutsideScope(ParseSpan),
    #[error("Scope close with {n_hashes} hashes encountered when closest scope open has {expected_closing_hashes}")]
    MismatchingScopeClose {
        n_hashes: usize,
        expected_closing_hashes: usize,
        scope_open_span: ParseSpan,
        scope_close_span: ParseSpan,
    },
    #[error("File ended inside code block")]
    EndedInsideCode { code_start: ParseSpan },
    #[error("File ended inside raw scope")]
    EndedInsideRawScope { raw_scope_start: ParseSpan },
    #[error("File ended inside scope")]
    EndedInsideScope { scope_start: ParseSpan },
    #[error("Block scope encountered mid-line")]
    BlockScopeOpenedMidPara { scope_start: ParseSpan },
    #[error("Inline scope contained paragraph break")]
    ParaBreakInInlineScope {
        scope_start: ParseSpan,
        para_break: ParseSpan,
    },
    // #[error("Python error: {pyerr}")]
    // PythonErr { pyerr: Py<PyErr>, code_span: ParseSpan }
    #[error("Block scope owner was not followed by a block scope")]
    BlockOwnerCodeHasNoScope { code_span: ParseSpan },
    #[error("Inline scope owner was not followed by an inline scope")]
    InlineOwnerCodeHasNoScope { code_span: ParseSpan },
}
impl InterpParaState {
    pub fn handle_token(
        &mut self,
        tok: TTToken,
        data: &str,
    ) -> anyhow::Result<(Option<InterpBlockAction>, Option<InterpSpecialAction>)> {
        let actions = self.compute_action(tok, data)?;
        self.handle_action(actions, data)
    }
    pub fn handle_action(
        &mut self,
        action: Option<InterpParaAction>,
        data: &str,
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
                    let content = todo!("create content in Python world?");
                    todo!("Put content into topmost scope or current sentence");
                    (S::MidLine, (None, None))
                }
                (S::MidLine, A::BreakSentence) => {
                    todo!("if self.sentence has items, push it into the paragraph");
                    (S::LineStart, (None, None))
                }

                (
                    S::LineStart | S::MidLine | S::AttachingInlineLevelCode { .. },
                    A::PushInlineScope(owner, span, n),
                ) => {
                    let scope = todo!("create python InlineScope");
                    self.inline_stack.push(scope);
                    (S::MidLine, (None, None))
                }
                (S::LineStart | S::MidLine, A::PopInlineScope(scope_close_span)) => {
                    let popped_scope = self.inline_stack.pop();
                    match popped_scope {
                        Some(popped_scope) => todo!("Insert popped_scope at the new topmost scope or in the current sentence"),
                        // TODO should specify *inline* scope, not all scopes
                        None => bail!(InterpError::ScopeCloseOutsideScope(scope_close_span))
                    }
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
                    if !self.inline_stack.is_empty() {
                        bail!(InterpError::ParaBreakInInlineScope {
                            scope_start: todo!("span for self.inline_stack.last()"),
                            para_break: end_para_span
                        })
                    }
                    todo!("finalize current sentence and push into paragraph");
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
                ScopeClose(_, _) => {
                    todo!("try to close inline scopes, complain if inline scopes empty")
                }

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
                ScopeClose(_, _) => {
                    todo!("try to close inline scopes, complain if inline scopes empty")
                }

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
                    Some(_code_span) => {
                        // The code ended...
                        let _eval_result = todo!("eval code and see if it's a block owner");
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
}
impl<'interp, 'a> InterpState<'interp, 'a> {
    fn new(ttpython: &'a mut TurnipTextPython<'interp>, data: &'a str) -> PyResult<Self> {
        let root: Py<BlockNode> = ttpython.with_gil(|py, _| Py::new(py, BlockNode::new()))?;
        Ok(Self {
            block_state: InterpBlockState::ReadyForNewBlock,
            comment_state: None,
            block_stack: vec![],
            root,
            ttpython,
            data,
        })
    }

    pub fn handle_token(&mut self, tok: TTToken) -> anyhow::Result<()> {
        let actions = self.compute_action(tok)?;
        self.handle_action(actions)
    }

    /// Return (block action, special action) to be executed in the order (block action, special action)
    fn compute_action(
        &mut self,
        tok: TTToken,
    ) -> anyhow::Result<(Option<InterpBlockAction>, Option<InterpSpecialAction>)> {
        use InterpBlockAction::*;
        use TTToken::*;

        // Handle comments separately
        if let Some(InterpCommentState { comment_start: _ }) = self.comment_state {
            let action = match tok {
                Newline(_) => Some(InterpSpecialAction::EndComment),
                _ => None,
            };
            // No change at the block level, potentially exit comment as a special action
            return Ok((None, action));
        }

        let action = match &mut self.block_state {
            InterpBlockState::ReadyForNewBlock => {
                match tok {
                    CodeOpen(span, n_hashes) => (Some(StartBlockLevelCode(span, n_hashes)), None),

                    // PushBlock with no code managing it
                    BlockScopeOpen(span, n_hashes) => (Some(PushBlock(None, span, n_hashes)), None),

                    // PushInlineScope with no code managing it
                    InlineScopeOpen(span, n_hashes) => (
                        Some(StartParagraph(Some(InterpParaAction::PushInlineScope(
                            None, span, n_hashes,
                        )))),
                        None,
                    ),

                    // StartRawBlock
                    RawScopeOpen(span, n_hashes) => (
                        Some(StartParagraph(Some(InterpParaAction::StartRawScope(
                            span, n_hashes,
                        )))),
                        None,
                    ),

                    // Try a scope close
                    ScopeClose(_, _) => todo!(),

                    // Complain - not in code mode
                    CodeClose(span, _) => bail!(InterpError::CodeCloseOutsideCode(span)),

                    // Do nothing - we're still ready to receive a new block
                    Newline(_) => (None, None),

                    // Enter comment mode
                    Hashes(span, _) => (None, Some(InterpSpecialAction::StartComment(span))),

                    // Normal text - start a new paragraph
                    _ => (
                        Some(StartParagraph(Some(InterpParaAction::PushInlineContent(
                            InlineNodeToCreate::UnescapedText(tok.stringify(self.data).into()),
                        )))),
                        None,
                    ),
                }
            }
            InterpBlockState::WritingPara(state) => state.handle_token(tok, self.data)?,
            InterpBlockState::BuildingBlockLevelCode {
                code,
                code_start,
                expected_n_hashes,
            } => {
                let code_span = compute_action_for_code_mode(
                    self.data,
                    tok,
                    code,
                    code_start,
                    *expected_n_hashes,
                );
                match code_span {
                    Some(_code_span) => {
                        // The code ended...
                        let _eval_result = todo!("eval code and see if it's a block owner");
                        // if eval_result is a BlockScopeOwner, WaitToAttachBlockCode
                        // elif eval_result is an InlineScopeOwner, WaitToAttachInlineCode
                        // else stringify and PushInlineContent
                    }
                    None => (None, None),
                }
            }
            InterpBlockState::AttachingBlockLevelCode { owner, code_span } => match tok {
                BlockScopeOpen(span, n_hashes) => {
                    (Some(PushBlock(Some(owner.clone()), span, n_hashes)), None)
                }
                _ => bail!(InterpError::BlockOwnerCodeHasNoScope {
                    code_span: *code_span
                }),
            },
        };

        Ok(action)
    }

    /// May recurse if StartParagraph(action)
    fn handle_action(
        &mut self,
        actions: (Option<InterpBlockAction>, Option<InterpSpecialAction>),
    ) -> anyhow::Result<()> {
        let (block_action, special_action) = actions;

        if let Some(action) = block_action {
            use InterpBlockAction as A;
            use InterpBlockState as S;
            let new_block_state = match (&self.block_state, action) {
                (S::ReadyForNewBlock, A::StartBlockLevelCode(code_start, expected_n_hashes)) => {
                    S::BuildingBlockLevelCode {
                        code: "".into(),
                        code_start,
                        expected_n_hashes,
                    }
                }
                (S::BuildingBlockLevelCode { .. }, A::WaitToAttachBlockCode(owner, code_span)) => {
                    S::AttachingBlockLevelCode { owner, code_span }
                }

                (
                    S::ReadyForNewBlock | S::BuildingBlockLevelCode { .. },
                    A::StartParagraph(action),
                ) => {
                    let para_state: InterpParaState =
                        todo!("New InterpParaState with LineStart as inl_state");
                    let requested_action = para_state.handle_action(action, self.data);
                    // TODO this should really only handle SpecialAction
                    self.handle_action(actions);
                    S::WritingPara(para_state)
                }
                (S::WritingPara(para_state), A::EndParagraph) => {
                    todo!("Push para_state.para onto the topmost entry in the block stack or the root");
                    S::ReadyForNewBlock
                }

                (
                    S::ReadyForNewBlock | S::AttachingBlockLevelCode { .. },
                    A::PushBlock(owner, scope_start, expected_n_hashes),
                ) => {
                    self.block_stack.push(InterpBlockScopeState {
                        node: todo!("Create python block node from owner"),
                        scope_start,
                        expected_n_hashes,
                    });
                    S::ReadyForNewBlock
                }
                (S::ReadyForNewBlock, A::PopBlock(scope_close_span)) => {
                    let popped_scope = self.block_stack.pop();
                    match popped_scope {
                        Some(popped_scope) => {
                            todo!("Insert popped_scope at the new topmost scope or in the document")
                        }
                        // TODO specify *block* scope
                        None => bail!(InterpError::ScopeCloseOutsideScope(scope_close_span)),
                    }
                }
                (_, action) => bail!(
                    "Invalid block state/action pair encountered ({0:?}, {1:?})",
                    self.block_state,
                    action
                ),
            };
            self.block_state = new_block_state;
        }

        if let Some(action) = special_action {
            match (&self.comment_state, action) {
                (Some(_), InterpSpecialAction::EndComment) => {
                    self.comment_state = None;
                }
                (None, InterpSpecialAction::StartComment(comment_start)) => {
                    self.comment_state = Some(InterpCommentState { comment_start })
                }
                (_, action) => bail!(
                    "Invalid special state/action pair encountered ({0:?}, {1:?})",
                    self.comment_state,
                    action
                ),
            }
        }

        Ok(())
    }
}

/// Returns Some(code_span) once the code has been closed
fn compute_action_for_code_mode(
    data: &str,
    tok: TTToken,
    code: &mut String,
    code_start: &ParseSpan,
    expected_n_hashes: usize,
) -> Option<ParseSpan> {
    match tok {
        TTToken::CodeClose(close_span, n) if n == expected_n_hashes => Some(ParseSpan {
            start: code_start.start,
            end: close_span.end,
        }),
        _ => {
            code.push_str(tok.stringify(data));
            None
        }
    }
}
