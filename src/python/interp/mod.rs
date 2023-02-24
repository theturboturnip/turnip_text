#![allow(dead_code)]

use pyo3::{
    prelude::*,
    types::{PyDict, PyString},
};
use thiserror::Error;

use crate::{
    lexer::{Escapable, TTToken},
    python::{interop::*, typeclass::PyTcRef},
    util::ParseSpan,
};

mod para;
use self::para::{InterpParaState, InterpParaTransition};

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
    pub fn new<'interp>(py: Python<'interp>, data: &'a str) -> InterpResult<Self> {
        let root = Py::new(py, BlockScope::new_rs(py, None)).err_as_interp_internal(py)?;
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
pub(crate) enum InterpBlockTransition {
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

    /// Start a paragraph, optionally executing a transition on the paragraph-level state machine
    ///
    /// - [InterpBlockState::ReadyForNewBlock] -> [InterpBlockState::WritingPara]
    /// - [InterpBlockState::BuildingBlockLevelCode] -> [InterpBlockState::WritingPara]
    /// TODO dear god stop using Option here, its so verbose and actually unnecessary
    StartParagraph(Option<InterpParaTransition>),

    /// On encountering a paragraph break (a blank line), add the paragraph to the document
    ///
    /// - [InterpBlockState::WritingPara] -> [InterpBlockState::ReadyForNewBlock]
    EndParagraph,

    /// On encountering a block scope close inside a paragraph,
    /// add the paragraph and close the topmost scope
    ///
    /// - [InterpBlockState::WritingPara] -> [InterpBlockState::ReadyForNewBlock]
    EndParagraphAndPopBlock(ParseSpan, usize),

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
pub(crate) enum InterpSpecialTransition {
    /// On encountering a comment starter, go into comment mode
    StartComment(ParseSpan),

    /// Leave comment mode
    EndComment,
}

#[derive(Debug)]
pub(crate) enum InlineNodeToCreate {
    UnescapedText(String),
    RawText(Option<PyTcRef<InlineScopeOwner>>, String),
    UnescapedPyString(Py<PyString>),
}
impl InlineNodeToCreate {
    fn to_py_intern(self, py: Python) -> PyResult<PyTcRef<InlineNode>> {
        let node = match self {
            InlineNodeToCreate::UnescapedText(s) => {
                let val = Py::new(py, UnescapedText::new_rs(py, s.as_str()))?;
                PyTcRef::of(val.as_ref(py))?
            }
            InlineNodeToCreate::RawText(owner, s) => {
                let val = Py::new(py, RawText::new_rs(py, owner, s.as_str()))?;
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
    SentenceBreakInInlineScope { scope_start: ParseSpan },
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
    #[error("Escaped newline (used for sentence continuation) found outside paragraph")]
    EscapedNewlineOutsideParagraph { newline: ParseSpan },
}

fn stringify_pyerr(py: Python, pyerr: &PyErr) -> String {
    let value = pyerr.value(py);
    let type_name = match value.get_type().name() {
        Ok(name) => name,
        Err(_) => "Unknown Type",
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
        self.map_err(|pyerr| InterpError::PythonErr {
            pyerr: stringify_pyerr(py, &pyerr),
            code_span,
        })
    }
    fn err_as_interp_internal(self, py: Python) -> InterpResult<T> {
        self.map_err(|pyerr| InterpError::InternalPythonErr {
            pyerr: stringify_pyerr(py, &pyerr),
        })
    }
}

impl<'a> InterpState<'a> {
    pub fn handle_token<'interp>(
        &mut self,
        py: Python<'interp>,
        globals: &'interp PyDict,
        tok: TTToken,
    ) -> InterpResult<()> {
        let transitions = self.mutate_and_find_transitions(py, globals, tok)?;
        self.handle_transition(py, globals, transitions)
    }

    pub fn finalize<'interp>(
        &mut self,
        py: Python<'interp>,
        globals: &'interp PyDict,
    ) -> InterpResult<()> {
        let transitions = match &mut self.block_state {
            InterpBlockState::ReadyForNewBlock => (None, None),
            InterpBlockState::WritingPara(state) => state.finalize(py)?,
            InterpBlockState::BuildingBlockLevelCode { code_start, .. } => {
                return Err(InterpError::EndedInsideCode {
                    code_start: *code_start,
                })
            }
            InterpBlockState::AttachingBlockLevelCode { code_span, .. } => {
                return Err(InterpError::BlockOwnerCodeHasNoScope {
                    code_span: *code_span,
                })
            }
        };

        match self.block_stack.pop() {
            // No open blocks on the stack => process the transition
            None => self.handle_transition(py, globals, transitions),
            Some(InterpBlockScopeState { scope_start, .. }) => {
                return Err(InterpError::EndedInsideScope { scope_start })
            }
        }
    }

    /// Return (block transition, special transition) to be executed in the order (block transition, special transition)
    fn mutate_and_find_transitions(
        &mut self,
        py: Python,
        globals: &PyDict,
        tok: TTToken,
    ) -> InterpResult<(
        Option<InterpBlockTransition>,
        Option<InterpSpecialTransition>,
    )> {
        use InterpBlockTransition::*;
        use TTToken::*;

        // Handle comments separately
        if let Some(InterpCommentState { comment_start: _ }) = self.comment_state {
            let transition = match tok {
                Newline(_) => Some(InterpSpecialTransition::EndComment),
                _ => None,
            };
            // No change at the block level, potentially exit comment as a special transition
            return Ok((None, transition));
        }

        let transition = match &mut self.block_state {
            InterpBlockState::ReadyForNewBlock => {
                match tok {
                    Escaped(span, Escapable::Newline) => {
                        return Err(InterpError::EscapedNewlineOutsideParagraph { newline: span })
                    }

                    CodeOpen(span, n_hashes) => (Some(StartBlockLevelCode(span, n_hashes)), None),

                    // PushBlock with no code managing it
                    BlockScopeOpen(span, n_hashes) => (Some(PushBlock(None, span, n_hashes)), None),

                    // PushInlineScope with no code managing it
                    InlineScopeOpen(span, n_hashes) => (
                        Some(StartParagraph(Some(InterpParaTransition::PushInlineScope(
                            None, span, n_hashes,
                        )))),
                        None,
                    ),

                    // StartRawBlock
                    RawScopeOpen(span, n_hashes) => (
                        Some(StartParagraph(Some(InterpParaTransition::StartRawScope(
                            None, span, n_hashes,
                        )))),
                        None,
                    ),

                    // Try a scope close
                    ScopeClose(span, n_hashes) => match self.block_stack.last() {
                        Some(InterpBlockScopeState {
                            expected_n_hashes, ..
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
                    Hashes(span, _) => (None, Some(InterpSpecialTransition::StartComment(span))),

                    // Normal text - start a new paragraph
                    _ => (
                        Some(StartParagraph(Some(InterpParaTransition::StartText(
                            tok.stringify_escaped(self.data).into(),
                        )))),
                        None,
                    ),
                }
            }
            InterpBlockState::WritingPara(state) => {
                state.handle_token(py, globals, tok, self.data)?
            }
            InterpBlockState::BuildingBlockLevelCode {
                code,
                code_start,
                expected_n_hashes,
            } => {
                let code_span =
                    handle_code_mode(self.data, tok, code, code_start, *expected_n_hashes);
                match code_span {
                    Some(code_span) => {
                        use EvalBracketResult::*;

                        // The code ended...
                        let res = EvalBracketResult::eval(py, globals, code.as_str())
                            .err_as_interp(py, code_span)?;
                        let block_transition = match res {
                            Block(b) => WaitToAttachBlockCode(b, code_span),
                            Inline(i) => StartParagraph(Some(
                                InterpParaTransition::WaitToAttachInlineCode(i, code_span),
                            )),
                            Other(s) => {
                                StartParagraph(Some(InterpParaTransition::PushInlineContent(
                                    InlineNodeToCreate::UnescapedPyString(s),
                                )))
                            }
                        };
                        (Some(block_transition), None)
                    }
                    None => (None, None),
                }
            }
            InterpBlockState::AttachingBlockLevelCode { owner, code_span } => match tok {
                BlockScopeOpen(span, n_hashes) => {
                    (Some(PushBlock(Some(owner.clone()), span, n_hashes)), None)
                }
                _ => {
                    return Err(InterpError::BlockOwnerCodeHasNoScope {
                        code_span: *code_span,
                    })
                }
            },
        };

        Ok(transition)
    }

    /// May recurse if StartParagraph(transition)
    fn handle_transition(
        &mut self,
        py: Python,
        globals: &PyDict,
        transitions: (
            Option<InterpBlockTransition>,
            Option<InterpSpecialTransition>,
        ),
    ) -> InterpResult<()> {
        let (block_transition, special_transition) = transitions;

        if let Some(transition) = block_transition {
            use InterpBlockState as S;
            use InterpBlockTransition as T;

            let new_block_state = match (&self.block_state, transition) {
                (S::ReadyForNewBlock, T::StartBlockLevelCode(code_start, expected_n_hashes)) => {
                    S::BuildingBlockLevelCode {
                        code: "".into(),
                        code_start,
                        expected_n_hashes,
                    }
                }
                (S::BuildingBlockLevelCode { .. }, T::WaitToAttachBlockCode(owner, code_span)) => {
                    S::AttachingBlockLevelCode { owner, code_span }
                }

                (
                    S::ReadyForNewBlock | S::BuildingBlockLevelCode { .. },
                    T::StartParagraph(transition),
                ) => {
                    let mut para_state = InterpParaState::new(py).err_as_interp_internal(py)?;
                    let (new_block_transition, new_special_transition) =
                        para_state.handle_transition(py, transition)?;
                    if new_block_transition.is_some() {
                        return Err(InterpError::InternalErr(
                            "An inline transition, initiated with the start of a paragraph, tried to initiate another block transition. This is not allowed and should not be possible.".into()
                        ));
                    }
                    self.handle_transition(py, globals, (None, new_special_transition))?;
                    S::WritingPara(para_state)
                }
                (S::WritingPara(para_state), T::EndParagraph) => {
                    self.push_to_topmost_block(py, para_state.para().as_ref(py))?;
                    S::ReadyForNewBlock
                }
                (
                    S::WritingPara(para_state),
                    T::EndParagraphAndPopBlock(scope_close_span, n_hashes),
                ) => {
                    // End paragraph i.e. push paragraph onto topmost block
                    self.push_to_topmost_block(py, para_state.para().as_ref(py))?;
                    // Pop block
                    let popped_scope = self.block_stack.pop();
                    match popped_scope {
                        Some(InterpBlockScopeState {
                            scope,
                            scope_start: scope_open_span,
                            expected_n_hashes,
                        }) => {
                            if n_hashes == expected_n_hashes {
                                // Push popped block into new topmost block
                                self.push_to_topmost_block(py, scope.as_ref(py))?
                            } else {
                                return Err(InterpError::MismatchingScopeClose {
                                    n_hashes,
                                    expected_n_hashes,
                                    scope_open_span,
                                    scope_close_span,
                                });
                            }
                        }
                        None => return Err(InterpError::ScopeCloseOutsideScope(scope_close_span)),
                    }
                    S::ReadyForNewBlock
                }

                (
                    S::ReadyForNewBlock | S::AttachingBlockLevelCode { .. },
                    T::PushBlock(owner, scope_start, expected_n_hashes),
                ) => {
                    self.block_stack.push(InterpBlockScopeState {
                        scope: Py::new(py, BlockScope::new_rs(py, owner))
                            .err_as_interp_internal(py)?,
                        scope_start,
                        expected_n_hashes,
                    });
                    S::ReadyForNewBlock
                }
                (S::ReadyForNewBlock, T::PopBlock(scope_close_span)) => {
                    let popped_scope = self.block_stack.pop();
                    match popped_scope {
                        Some(popped_scope) => {
                            self.push_to_topmost_block(py, popped_scope.scope.as_ref(py))?
                        }
                        None => return Err(InterpError::ScopeCloseOutsideScope(scope_close_span)),
                    }
                    S::ReadyForNewBlock
                }
                (_, transition) => {
                    return Err(InterpError::InternalErr(format!(
                        "Invalid block state/transition pair encountered ({0:?}, {1:?})",
                        self.block_state, transition
                    )))
                }
            };
            self.block_state = new_block_state;
        }

        if let Some(transition) = special_transition {
            match (&self.comment_state, transition) {
                (Some(_), InterpSpecialTransition::EndComment) => {
                    self.comment_state = None;
                }
                (None, InterpSpecialTransition::StartComment(comment_start)) => {
                    self.comment_state = Some(InterpCommentState { comment_start })
                }
                (_, transition) => {
                    return Err(InterpError::InternalErr(format!(
                        "Invalid special state/transition pair encountered ({0:?}, {1:?})",
                        self.comment_state, transition
                    )))
                }
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
        }
        .err_as_interp_internal(py)
    }
}

/// Returns Some(code_span) once the code has been closed
fn handle_code_mode(
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
