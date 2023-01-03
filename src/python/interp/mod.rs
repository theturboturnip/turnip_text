#![allow(dead_code)]

use thiserror::Error;
use pyo3::{prelude::*, types::{PyString, PyDict}};

use crate::{lexer::TTToken, util::ParseSpan, python::{interop::*, TurnipTextPython, typeclass::PyTcRef}};

mod para;
use self::para::{InterpParaState, InterpParaAction};

pub struct InterpState<'a> {
    /// FSM state
    block_state: InterpBlockState,
    /// Overrides InterpBlockState and raw_state - if Some(state), we are in "comment mode" and all other state machines are paused
    comment_state: Option<InterpCommentState>,
    /// Stack of block scopes
    block_stack: Vec<InterpBlockScopeState>,
    /// Root of the document
    root: Py<BlockScope>,
    /// Raw text data
    data: &'a str,
}
impl<'a> InterpState<'a> {
   pub fn new<'interp>(ttpython: &'a TurnipTextPython<'interp>, data: &'a str) -> InterpResult<Self> {
        let root = ttpython.with_gil(
            |py, _| Py::new(py, BlockScope::new_rs(py, None)).err_as_interp_internal(py)
        )?;
        Ok(Self {
            block_state: InterpBlockState::ReadyForNewBlock,
            comment_state: None,
            block_stack: vec![],
            root,
            data,
        })
    }
    pub fn root(&self) -> Py<BlockScope> {
        self.root.clone()
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
        owner: PyTcRef<BlockScopeOwner>,
        code_span: ParseSpan,
    },
}


#[derive(Debug)]
struct InterpCommentState {
    comment_start: ParseSpan,
}

#[derive(Debug)]
struct InterpBlockScopeState {
    scope: Py<BlockScope>,
    scope_start: ParseSpan,
    expected_n_hashes: usize,
}

#[derive(Debug)]
pub(crate) enum InterpBlockAction {
    /// On encountering a CodeOpen at the start of a line, start gathering block level code
    ///
    /// - [InterpBlockState::ReadyForNewBlock] -> [InterpBlockState::BuildingBlockLevelCode]
    /// - All others invalid
    StartBlockLevelCode(ParseSpan, usize),

    /// Having finished a block-level code close which evals to [BlockScopeOwner],
    /// start a one-token wait for an inline scope to attach it to
    ///
    /// - [InterpBlockState::BuildingBlockLevelCode] -> [InterpBlockState::AttachingBlockLevelCode]
    WaitToAttachBlockCode(PyTcRef<BlockScopeOwner>, ParseSpan),

    /// Start a paragraph, optionally executing an action on the paragraph-level state machine
    ///
    /// - [InterpBlockState::ReadyForNewBlock] -> [InterpBlockState::WritingPara]
    /// - [InterpBlockState::BuildingBlockLevelCode] -> [InterpBlockState::WritingPara]
    /// TODO dear god stop using Option here, its so verbose and actually unnecessary
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
    PushBlock(Option<PyTcRef<BlockScopeOwner>>, ParseSpan, usize),

    /// On encountering a scope close, pop the current block from the scope
    ///
    /// - [InterpBlockState::ReadyForNewBlock] -> [InterpBlockState::ReadyForNewBlock]
    PopBlock(ParseSpan),
}

#[derive(Debug)]
pub(crate) enum InterpSpecialAction {
    /// On encountering a comment starter, go into comment mode
    StartComment(ParseSpan),

    /// Leave comment mode
    EndComment,
}

#[derive(Debug)]
pub(crate) enum InlineNodeToCreate {
    UnescapedText(String),
    RawText(String),
    UnescapedPyString(Py<PyString>)
}
impl InlineNodeToCreate {
    fn to_py_intern(self, py: Python) -> PyResult<PyTcRef<InlineNode>> {
        let node = match self {
            InlineNodeToCreate::UnescapedText(s) => {
                let val = Py::new(py, UnescapedText::new_rs(py, s.as_str()))?;
                PyTcRef::of(val.as_ref(py))?
            }
            InlineNodeToCreate::RawText(s) => {
                let val = Py::new(py, RawText::new_rs(py, s.as_str()))?;
                PyTcRef::of(val.as_ref(py))?
            }
            InlineNodeToCreate::UnescapedPyString(s) => {
                let val = Py::new(py, UnescapedText::new(s))?;
                PyTcRef::of(val.as_ref(py))?
            }
        };
        Ok(node)
    }
    pub(crate) fn to_py(self, py: Python) -> InterpResult<PyTcRef<InlineNode>> {
        self.to_py_intern(py).err_as_interp_internal(py)
    }
}

/// Enumeration of all possible interpreter errors
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InterpError {
    #[error("Code close encountered outside of code mode")]
    CodeCloseOutsideCode(ParseSpan),
    #[error("Scope close encountered with no matching scope open")]
    ScopeCloseOutsideScope(ParseSpan),
    #[error("Scope close with {n_hashes} hashes encountered when closest scope open has {expected_n_hashes}")]
    MismatchingScopeClose {
        n_hashes: usize,
        expected_n_hashes: usize,
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
    #[error("A Python `BlockScopeOwner` was returned by inline code inside a paragraph")]
    BlockOwnerCodeMidPara { code_span: ParseSpan },
    #[error("Inline scope contained sentence break")]
    SentenceBreakInInlineScope {
        scope_start: ParseSpan
    },
    #[error("Inline scope contained paragraph break")]
    ParaBreakInInlineScope {
        scope_start: ParseSpan,
        para_break: ParseSpan,
    },
    #[error("Block scope owner was not followed by a block scope")]
    BlockOwnerCodeHasNoScope { code_span: ParseSpan },
    #[error("Inline scope owner was not followed by an inline scope")]
    InlineOwnerCodeHasNoScope { code_span: ParseSpan },
    #[error("Python error: {pyerr}")]
    PythonErr { pyerr: String, code_span: ParseSpan },
    #[error("Internal python error: {pyerr}")]
    InternalPythonErr { pyerr: String },
    #[error("Internal error: {0}")]
    InternalErr(String),
}

fn stringify_pyerr(py: Python, pyerr: &PyErr) -> String {
    let value = pyerr.value(py);
    let type_name = match value.get_type().name() {
        Ok(name) => name,
        Err(_) => "Unknown Type"
    };
    if let Ok(s) = value.str() {
        format!("{0} : {1}", type_name, &s.to_string_lossy())
    } else {
        "<exception str() failed>".into()
    }
}

pub type InterpResult<T> = Result<T, InterpError>;

trait MapInterpResult<T> {
    fn err_as_interp(self, py: Python, code_span: ParseSpan) -> InterpResult<T>;
    fn err_as_interp_internal(self, py: Python) -> InterpResult<T>;
}
impl<T> MapInterpResult<T> for PyResult<T> {
    fn err_as_interp(self, py: Python, code_span: ParseSpan) -> InterpResult<T> {
        self.map_err(|pyerr| {
            InterpError::PythonErr{
                pyerr: stringify_pyerr(py, &pyerr),
                code_span
            }
        })
    }
    fn err_as_interp_internal(self, py: Python) -> InterpResult<T> {
        self.map_err(|pyerr| {
            InterpError::InternalPythonErr {
                pyerr: stringify_pyerr(py, &pyerr),
            }
        })
    }
}

impl<'a> InterpState<'a> {
    pub fn handle_token<'interp>(&mut self, ttpython: &TurnipTextPython<'interp>, tok: TTToken) -> InterpResult<()> {
        ttpython.with_gil(|py, globals| {
            let actions = self.compute_action(py, globals, tok)?;
            self.handle_action(py, globals, actions)
        })
    }

    pub fn finalize<'interp>(&mut self, ttpython: &TurnipTextPython<'interp>) -> InterpResult<()> {
        ttpython.with_gil(|py, globals| {
            let actions = match &mut self.block_state {
                InterpBlockState::ReadyForNewBlock => {
                    (None, None)
                },
                InterpBlockState::WritingPara(state) => state.finalize(py)?,
                InterpBlockState::BuildingBlockLevelCode { code_start, .. } => return Err(InterpError::EndedInsideCode { code_start: *code_start }),
                InterpBlockState::AttachingBlockLevelCode { code_span, .. } => return Err(InterpError::BlockOwnerCodeHasNoScope { code_span: *code_span }),
            };

            match self.block_stack.pop() {
                // No open blocks on the stack => process the action
                None => self.handle_action(py, globals, actions),
                Some(InterpBlockScopeState{scope_start, ..}) => return Err(InterpError::EndedInsideScope { scope_start })
            }
        })
    }

    /// Return (block action, special action) to be executed in the order (block action, special action)
    fn compute_action(
        &mut self,
        py: Python,
        globals: &PyDict,
        tok: TTToken,
    ) -> InterpResult<(Option<InterpBlockAction>, Option<InterpSpecialAction>)> {
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
                    ScopeClose(span, n_hashes) => match self.block_stack.last() {
                        Some(InterpBlockScopeState {
                            expected_n_hashes,
                            ..
                        }) if n_hashes == *expected_n_hashes => (Some(PopBlock(span)), None),
                        Some(InterpBlockScopeState {
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

                    // Complain - not in code mode
                    CodeClose(span, _) => return Err(InterpError::CodeCloseOutsideCode(span)),

                    // Do nothing - we're still ready to receive a new block
                    Newline(_) => (None, None),

                    // Enter comment mode
                    Hashes(span, _) => (None, Some(InterpSpecialAction::StartComment(span))),

                    // Normal text - start a new paragraph
                    _ => (
                        Some(StartParagraph(Some(InterpParaAction::StartText(
                            tok.stringify_escaped(self.data).into(),
                        )))),
                        None,
                    ),
                }
            }
            InterpBlockState::WritingPara(state) => {
                state.handle_token(py, globals, tok, self.data)?
            },
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
                    Some(code_span) => {
                        use EvalBracketResult::*;

                        // The code ended...
                        let res = EvalBracketResult::eval(
                            py, globals, code.as_str()
                        ).err_as_interp(py, code_span)?;
                        let block_action = match res {
                            Block(b) => WaitToAttachBlockCode(b, code_span),
                            Inline(i) => StartParagraph(Some(InterpParaAction::WaitToAttachInlineCode(i, code_span))),
                            Other(s) => StartParagraph(Some(InterpParaAction::PushInlineContent(InlineNodeToCreate::UnescapedPyString(s)))),
                        };
                        (Some(block_action), None)
                    }
                    None => (None, None),
                }
            }
            InterpBlockState::AttachingBlockLevelCode { owner, code_span } => match tok {
                BlockScopeOpen(span, n_hashes) => {
                    (Some(PushBlock(Some(owner.clone()), span, n_hashes)), None)
                }
                _ => return Err(InterpError::BlockOwnerCodeHasNoScope {
                    code_span: *code_span
                }),
            },
        };

        Ok(action)
    }

    /// May recurse if StartParagraph(action)
    fn handle_action(
        &mut self,
        py: Python,
        globals: &PyDict,
        actions: (Option<InterpBlockAction>, Option<InterpSpecialAction>),
    ) -> InterpResult<()> {
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
                    let mut para_state = InterpParaState::new(py).err_as_interp_internal(py)?;
                    let (new_block_action, new_special_action) = para_state.handle_action(py, action)?;
                    if new_block_action.is_some() {
                        return Err(InterpError::InternalErr(
                            "An inline action, initiated with the start of a paragraph, tried to initiate another block action. This is not allowed and should not be possible.".into()
                        ))
                    }
                    self.handle_action(py, globals, (None, new_special_action))?;
                    S::WritingPara(para_state)
                }
                (
                    S::WritingPara(para_state),
                    A::EndParagraph
                ) => {
                    self.push_to_topmost_block(py, para_state.para().as_ref(py))?;
                    S::ReadyForNewBlock
                }

                (
                    S::ReadyForNewBlock | S::AttachingBlockLevelCode { .. },
                    A::PushBlock(owner, scope_start, expected_n_hashes),
                ) => {
                    self.block_stack.push(InterpBlockScopeState {
                        scope: Py::new(py, BlockScope::new_rs(py, owner)).err_as_interp_internal(py)?,
                        scope_start,
                        expected_n_hashes,
                    });
                    S::ReadyForNewBlock
                }
                (
                    S::ReadyForNewBlock,
                    A::PopBlock(scope_close_span)
                ) => {
                    let popped_scope = self.block_stack.pop();
                    match popped_scope {
                        Some(popped_scope) => {
                            self.push_to_topmost_block(py, popped_scope.scope.as_ref(py))?
                        }
                        // TODO specify *block* scope
                        None => return Err(InterpError::ScopeCloseOutsideScope(scope_close_span)),
                    }
                    S::ReadyForNewBlock
                }
                (_, action) => return Err(
                    InterpError::InternalErr(
                        format!(
                            "Invalid block state/action pair encountered ({0:?}, {1:?})",
                            self.block_state,
                            action
                        )
                    )
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
                (_, action) => return Err(
                    InterpError::InternalErr(
                        format!(
                            "Invalid special state/action pair encountered ({0:?}, {1:?})",
                            self.comment_state,
                            action
                        )
                    )
                ),
            }
        }

        Ok(())
    }

    fn push_to_topmost_block(&self, py: Python, node: &PyAny) -> InterpResult<()> {
        {
            let pyref = match self.block_stack.last() {
                Some(b) => &b.scope,
                None => &self.root,
            };
            let scope = &mut *pyref.borrow_mut(py);
            scope.push_node(node)
        }.err_as_interp_internal(py)
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
            // Code blocks use raw stringification to avoid confusion between text written and text entered
            code.push_str(tok.stringify_raw(data));
            None
        }
    }
}
