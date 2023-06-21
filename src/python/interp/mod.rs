#![allow(dead_code)]

use pyo3::{prelude::*, types::PyDict};
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
        let root = Py::new(py, BlockScope::new_empty(py)).err_as_interp_internal(py)?;
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
    /// Transitions to [Self::AttachingBlockLevelCode] or [Self::ReadyForNewBlock] once finished
    BuildingCode {
        code: String,
        code_start: ParseSpan,
        expected_close_len: usize,
    },
}

#[derive(Debug)]
struct InterpCommentState {
    comment_start: ParseSpan,
}

#[derive(Debug)]
struct InterpBlockScopeState {
    builder: Option<PyTcRef<BlockScopeBuilder>>,
    children: Py<BlockScope>,
    scope_start: ParseSpan,
}
impl InterpBlockScopeState {
    fn build_to_block(self, py: Python, scope_end: ParseSpan) -> InterpResult<PyTcRef<Block>> {
        let scope = ParseSpan::new(self.scope_start.start, scope_end.end);
        match self.builder {
            Some(builder) => BlockScopeBuilder::call_build_from_blocks(py, builder, self.children)
                .err_as_interp(py, scope),
            None => Ok(PyTcRef::of(self.children.as_ref(py)).expect("Internal error: InterpBlockScopeState::children, a BlockScope, somehow doesn't fit the Block typeclass")),
        }
    }
}

#[derive(Debug)]
pub(crate) enum InterpBlockTransition {
    /// On encountering a CodeOpen at the start of a line, start gathering block level code
    ///
    /// - [InterpBlockState::ReadyForNewBlock] -> [InterpBlockState::BuildingBlockLevelCode]
    /// - All others invalid
    StartBlockLevelCode(ParseSpan, usize),

    /// Start a paragraph, optionally executing a transition on the paragraph-level state machine
    ///
    /// - [InterpBlockState::ReadyForNewBlock] -> [InterpBlockState::WritingPara]
    /// - [InterpBlockState::BuildingCode] -> [InterpBlockState::WritingPara]
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
    EndParagraphAndPopBlock(ParseSpan),

    /// On encountering a block scope owner (i.e. a BlockScopeOpen optionally preceded by a Python scope owner),
    /// push a block scope onto the current stack
    ///
    /// - [InterpBlockState::ReadyForNewBlock] -> [InterpBlockState::ReadyForNewBlock]
    /// - [InterpBlockState::BuildingCode] -> [InterpBlockState::ReadyForNewBlock]
    PushBlockScope(Option<PyTcRef<BlockScopeBuilder>>, ParseSpan),

    /// If an eval-bracket emits a Block directly, push it onto the stack
    /// 
    /// - [InterpBlockState::BuildingCode] -> [InterpBlockState::ReadyForNewBlock]
    PushBlock(PyTcRef<Block>),

    /// On encountering a scope close, pop the current block from the scope
    ///
    /// - [InterpBlockState::ReadyForNewBlock] -> [InterpBlockState::ReadyForNewBlock]
    PopBlockScope(ParseSpan),
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
    RawText(Option<PyTcRef<RawScopeBuilder>>, String),
    PythonObject(PyTcRef<Inline>),
}
impl InlineNodeToCreate {
    fn to_py_intern(self, py: Python) -> PyResult<PyTcRef<Inline>> {
        match self {
            InlineNodeToCreate::UnescapedText(s) => {
                let unescaped_text = Py::new(py, UnescapedText::new_rs(py, s.as_str()))?;
                PyTcRef::of(unescaped_text.as_ref(py))
            }
            InlineNodeToCreate::RawText(builder, raw) => match builder {
                Some(builder) => RawScopeBuilder::call_build_from_raw(py, builder, raw),
                None => {
                    let raw_text = Py::new(py, RawText::new_rs(py, raw.as_str()))?;
                    PyTcRef::of(raw_text.as_ref(py))
                }
            },
            InlineNodeToCreate::PythonObject(obj) => Ok(obj),
        }
    }
    pub(crate) fn to_py(self, py: Python) -> InterpResult<PyTcRef<Inline>> {
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
    #[error("Raw scope close when not in a raw scope")]
    RawScopeCloseOutsideRawScope(ParseSpan),
    #[error("File ended inside code block")]
    EndedInsideCode { code_start: ParseSpan },
    #[error("File ended inside raw scope")]
    EndedInsideRawScope { raw_scope_start: ParseSpan },
    #[error("File ended inside scope")]
    EndedInsideScope { scope_start: ParseSpan },
    #[error("Block scope encountered mid-line")]
    BlockScopeOpenedMidPara { scope_start: ParseSpan },
    #[error("A Python `BlockScopeOwner` was returned by code inside a paragraph")]
    BlockOwnerCodeMidPara { code_span: ParseSpan },
    #[error("A Python `Block` was returned by code inside a paragraph")]
    BlockCodeMidPara { code_span: ParseSpan },
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
            InterpBlockState::BuildingCode { code_start, .. } => {
                return Err(InterpError::EndedInsideCode {
                    code_start: *code_start,
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
                    BlockScopeOpen(span) => (Some(PushBlockScope(None, span)), None),

                    // PushInlineScope with no code managing it
                    InlineScopeOpen(span) => (
                        Some(StartParagraph(Some(InterpParaTransition::PushInlineScope(
                            None, span,
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
                    RawScopeClose(span, _) => Err(InterpError::RawScopeCloseOutsideRawScope(span))?,

                    // Try a scope close
                    ScopeClose(span) => match self.block_stack.last() {
                        Some(_) => (Some(PopBlockScope(span)), None),
                        None => Err(InterpError::ScopeCloseOutsideScope(span))?,
                    },

                    // Complain - not in code mode
                    CodeClose(span, _) => return Err(InterpError::CodeCloseOutsideCode(span)),

                    // Do nothing - we're still ready to receive a new block
                    Newline(_) => (None, None),
                    // Ignore whitespace at the start of a paragraph
                    Whitespace(_) => (None, None),

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
            InterpBlockState::BuildingCode {
                code,
                code_start,
                expected_close_len,
            } => {
                match handle_code_mode(
                    self.data,
                    tok,
                    code,
                    code_start,
                    *expected_close_len,
                    py,
                    globals,
                )? {
                    Some((res, code_span)) => {
                        use EvalBracketResult::*;

                        let block_transition = match res {
                            BlockBuilder(b) => PushBlockScope(Some(b), code_span),
                            InlineBuilder(i) => StartParagraph(Some(
                                InterpParaTransition::PushInlineScope(Some(i), code_span),
                            )),
                            Raw(r, n_hashes) => StartParagraph(Some(
                                InterpParaTransition::StartRawScope(Some(r), code_span, n_hashes),
                            )),
                            Block(b) => PushBlock(b),
                            Inline(i) => {
                                StartParagraph(Some(InterpParaTransition::PushInlineContent(
                                    InlineNodeToCreate::PythonObject(
                                        i
                                    ),
                                )))
                            }
                        };
                        (Some(block_transition), None)
                    }
                    None => (None, None),
                }
            }
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
                (S::ReadyForNewBlock, T::StartBlockLevelCode(code_start, expected_close_len)) => {
                    S::BuildingCode {
                        code: "".into(),
                        code_start,
                        expected_close_len,
                    }
                }

                (
                    S::ReadyForNewBlock | S::BuildingCode { .. },
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
                (S::WritingPara(para_state), T::EndParagraphAndPopBlock(scope_close_span)) => {
                    // End paragraph i.e. push paragraph onto topmost block
                    self.push_to_topmost_block(py, para_state.para().as_ref(py))?;
                    // Pop block
                    let popped_scope = self.block_stack.pop();
                    match popped_scope {
                        Some(popped_scope) => {
                            let block = popped_scope.build_to_block(py, scope_close_span)?;
                            self.push_to_topmost_block(py, block.as_ref(py))?
                        }
                        None => return Err(InterpError::ScopeCloseOutsideScope(scope_close_span)),
                    }
                    S::ReadyForNewBlock
                }

                (
                    S::BuildingCode { .. },
                    T::PushBlock(b)
                ) => {
                    self.push_to_topmost_block(py, b.as_ref(py))?;
                    S::ReadyForNewBlock
                }
                (
                    S::ReadyForNewBlock | S::BuildingCode { .. },
                    T::PushBlockScope(builder, scope_start),
                ) => {
                    self.block_stack.push(InterpBlockScopeState {
                        builder,
                        children: Py::new(py, BlockScope::new_empty(py))
                            .err_as_interp_internal(py)?,
                        scope_start,
                    });
                    S::ReadyForNewBlock
                }
                (S::ReadyForNewBlock, T::PopBlockScope(scope_close_span)) => {
                    let popped_scope = self.block_stack.pop();
                    match popped_scope {
                        Some(popped_scope) => {
                            let block = popped_scope.build_to_block(py, scope_close_span)?;
                            self.push_to_topmost_block(py, block.as_ref(py))?
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

    fn push_to_topmost_block(&self, py: Python, block: &PyAny) -> InterpResult<()> {
        {
            let child_list_ref = match self.block_stack.last() {
                Some(b) => &b.children,
                None => &self.root,
            };
            child_list_ref.borrow_mut(py).push_block(block)
        }
        .err_as_interp_internal(py)
    }
}

mod eval_bracket;
use eval_bracket::{EvalBracketResult, handle_code_mode};