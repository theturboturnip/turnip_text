use pyo3::{prelude::*, types::PyDict};
use thiserror::Error;

use crate::{
    error::{stringify_pyerr, TurnipTextContextlessError, TurnipTextContextlessResult},
    lexer::{Escapable, LexError, TTToken},
    util::ParseSpan,
};

mod para;
use self::para::{InterpParaState, InterpParaTransition};

mod eval_bracket;
use eval_bracket::{eval_brackets, EvalBracketResult};

pub mod python;
use python::{
    interop::*,
    typeclass::{PyInstanceList, PyTcRef, PyTcUnionRef},
};

pub mod next;

pub struct Interpreter {
    /// FSM state
    block_state: InterpBlockState,
    /// Overrides InterpBlockState and raw_state - if Some(state), we are in "comment mode" and all other state machines are paused
    comment_state: Option<InterpCommentState>,
    /// The current structure of the document, including toplevel content, segments, and the current block stacks (one block stack per included subfile)
    structure: InterimDocumentStructure,
}
impl Interpreter {
    pub fn new(py: Python) -> PyResult<Self> {
        Ok(Self {
            block_state: InterpBlockState::ReadyForNewBlock,
            comment_state: None,
            structure: InterimDocumentStructure::new(py)?,
        })
    }

    pub fn handle_tokens<'a>(
        &'a mut self,
        py: Python,
        py_env: &PyDict,
        toks: &mut impl Iterator<Item = Result<TTToken, LexError>>,
        file_idx: usize, // Attached to any LexError given
        data: &str,
    ) -> TurnipTextContextlessResult<InterpreterFileAction> {
        for tok in toks {
            let tok = tok.map_err(|lex_err| (file_idx, lex_err))?;
            let transitions = self.mutate_and_find_transitions(py, py_env, tok, data)?;
            match self.handle_transition(py, py_env, transitions)? {
                None => continue,
                Some(TurnipTextSource { name, contents }) => {
                    return Ok(InterpreterFileAction::FileInserted { name, contents })
                }
            }
        }
        Ok(InterpreterFileAction::FileEnded)
    }
}

pub struct InterimDocumentStructure {
    /// Top level content of the document
    /// All text leading up to the first DocSegment
    toplevel_content: Py<BlockScope>,
    /// The top level segments
    toplevel_segments: PyInstanceList<DocSegment>,
    /// The stack of DocSegments leading up to the current doc segment.
    segment_stack: Vec<InterpDocSegmentState>,
    /// A stack of the pieces of data we're parsing from.
    /// Each parsed file has its own block stack - we don't allow a given subfile to leave new scopes open, or close scopes it didn't open.
    block_stacks: Vec<Vec<InterpManualBlockScopeState>>,
}
impl InterimDocumentStructure {
    pub fn new(py: Python) -> PyResult<Self> {
        Ok(Self {
            toplevel_content: Py::new(py, BlockScope::new_empty(py))?,
            toplevel_segments: PyInstanceList::new(py),
            segment_stack: vec![],
            block_stacks: vec![],
        })
    }

    pub fn finalize(self, py: Python) -> PyResult<Py<DocSegment>> {
        assert_eq!(
            self.block_stacks.len(),
            0,
            "Tried to finalize the document while inside a file"
        );
        assert_eq!(
            self.segment_stack.len(),
            0,
            "Tried to finalize the document with in-progress segments?"
        );
        Py::new(
            py,
            DocSegment::new_no_header(
                py,
                self.toplevel_content.clone(),
                self.toplevel_segments.clone(),
            )?,
        )
    }

    // TODO check_can_start_subfile? Right now we assume we only emit IncludedFiles in block mode, but having a sanity check for "not in paragraph mode" might be good

    fn check_can_end_subfile(&mut self) -> TurnipTextContextlessResult<()> {
        // Don't pop it immediately - if we return an error we need to keep this data around
        let top_block_stack = self
            .block_stacks
            .last()
            .expect("Tried to check_file_end when no data is left!");

        if let Some(block) = top_block_stack.last() {
            Err(InterpError::EndedInsideScope {
                scope_start: block.scope_start,
            }
            .into()) // TODO make this error specific to subfiles?
        } else {
            Ok(())
        }
    }

    /// This function returns the closest open block scope, which may have been opened in the current file or previous files.
    /// If there isn't any open block scope, returns None.
    fn get_enclosing_block(&self) -> Option<&InterpManualBlockScopeState> {
        for block_stack in self.block_stacks.iter().rev() {
            match block_stack.last() {
                Some(block) => return Some(block),
                None => {}
            }
        }
        None
    }

    fn push_enclosing_block_within_data(&mut self, block: InterpManualBlockScopeState) {
        self.block_stacks
            .last_mut()
            .expect("Must always have at least one block_stack")
            .push(block)
    }

    fn pop_enclosing_block_within_data(&mut self) -> Option<InterpManualBlockScopeState> {
        self.block_stacks
            .last_mut()
            .expect("Must always have at least one block_stack")
            .pop()
    }

    fn push_segment_header(
        &mut self,
        py: Python,
        header: PyTcRef<DocSegmentHeader>,
        code_span: ParseSpan,
        block_close_span: Option<ParseSpan>,
    ) -> TurnipTextContextlessResult<()> {
        if let Some(enclosing_block) = self.get_enclosing_block() {
            return Err(InterpError::DocSegmentHeaderMidScope {
                code_span,
                block_close_span,
                enclosing_scope_start: enclosing_block.scope_start,
            }
            .into());
        }

        let subsegment_weight =
            DocSegmentHeader::get_weight(py, header.as_ref(py)).err_as_internal(py)?;

        // If there are items in the segment_stack, pop from self.segment_stack until the toplevel weight < subsegment_weight
        self.pop_segments_until_less_than(py, subsegment_weight)?;

        // We know the thing at the top of the segment stack has a weight < subsegment_weight
        // Push pending segment state to the stack
        let subsegment =
            InterpDocSegmentState::new(py, header, subsegment_weight).err_as_internal(py)?;
        self.segment_stack.push(subsegment);

        Ok(())
    }

    fn pop_segments_until_less_than(
        &mut self,
        py: Python,
        weight: i64,
    ) -> TurnipTextContextlessResult<()> {
        let mut curr_toplevel_weight = match self.segment_stack.last() {
            Some(segment) => segment.weight,
            None => return Ok(()),
        };

        // We only get to this point if self.segment_stack.last() == Some
        while curr_toplevel_weight >= weight {
            let segment_to_finish = self
                .segment_stack
                .pop()
                .expect("Just checked, it isn't empty");

            let segment = segment_to_finish.finish(py).err_as_internal(py)?;

            // Look into the next segment
            curr_toplevel_weight = match self.segment_stack.last() {
                // If there's another segment, push the new finished segment into it
                Some(x) => {
                    x.subsegments
                        .append_checked(segment.as_ref(py))
                        .err_as_internal(py)?;
                    x.weight
                }
                // Otherwise just push it into toplevel_segments and FUCK, NO, THAT DOESN'T WORK YOU MORON
                // WHERE THE FUCK DOES THE NEXT SET OF TEXT GO THEN
                // TODO resolve whatever the hell was the problem here?
                None => {
                    self.toplevel_segments
                        .append_checked(segment.as_ref(py))
                        .err_as_internal(py)?;
                    return Ok(());
                }
            };
        }

        Ok(())
    }

    fn push_to_topmost_block(&self, py: Python, block: &PyAny) -> TurnipTextContextlessResult<()> {
        {
            // Figure out which block is actually the topmost block.
            // If the block stack has elements, add it to the topmost element.
            // If the block stack is empty, and there are elements on the DocSegment stack, take the topmost element of the DocSegment stack
            // If the block stack is empty and the segment stack is empty add to toplevel content.
            let child_list_ref = match self.get_enclosing_block() {
                Some(b) => &b.children,
                None => match self.segment_stack.last() {
                    Some(segment) => &segment.content,
                    None => &self.toplevel_content,
                },
            };
            child_list_ref.borrow_mut(py).push_block(block)
        }
        .err_as_internal(py)
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
    /// When building raw text initiated at the block level, owned by a RawScopeBuilder, which may return Inline or Block content
    BuildingRawText {
        builder: PyTcRef<RawScopeBuilder>,
        text: String,
        builder_span: ParseSpan,
        expected_n_hashes: usize,
    },
}

#[derive(Debug)]
struct InterpCommentState {
    comment_start: ParseSpan,
}

#[derive(Debug)]
struct InterpDocSegmentState {
    header: PyTcRef<DocSegmentHeader>,
    weight: i64,
    content: Py<BlockScope>,
    subsegments: PyInstanceList<DocSegment>,
}
impl InterpDocSegmentState {
    fn new(py: Python, header: PyTcRef<DocSegmentHeader>, weight: i64) -> PyResult<Self> {
        Ok(Self {
            header,
            weight,
            content: Py::new(py, BlockScope::new_empty(py))?,
            subsegments: PyInstanceList::new(py),
        })
    }
    fn finish(self, py: Python) -> PyResult<Py<DocSegment>> {
        Py::new(
            py,
            DocSegment::new_checked(py, Some(self.header), self.content, self.subsegments)?,
        )
    }
}

#[derive(Debug)]
struct InterpManualBlockScopeState {
    builder: Option<(PyTcRef<BlockScopeBuilder>, ParseSpan)>,
    children: Py<BlockScope>,
    scope_start: ParseSpan,
}
impl InterpManualBlockScopeState {
    fn build_to_block(
        self,
        py: Python,
        scope_end: ParseSpan,
    ) -> TurnipTextContextlessResult<(
        Option<PyTcUnionRef<Block, DocSegmentHeader>>,
        Option<ParseSpan>,
    )> {
        let scope = self.scope_start.combine(&scope_end);
        match self.builder {
            Some((builder, code_span)) => {
                let block_or_header =
                    BlockScopeBuilder::call_build_from_blocks(py, builder, self.children)
                        .err_as_interp(
                            py,
                            "Error while calling .build_from_blocks() on object",
                            scope,
                        )?;
                Ok((block_or_header, Some(code_span)))
            }
            None => {
                Ok((
                    Some(
                        PyTcUnionRef::of_friendly(self.children.as_ref(py), "internal").expect("Internal error: InterpBlockScopeState::children, a BlockScope, somehow doesn't fit the Block typeclass")
                    ),
                    None // Wasn't created with code
                ))
            }
        }
    }
}

#[derive(Debug)]
pub(crate) enum InterpBlockTransition {
    /// On encountering a CodeOpen at the start of a line, start gathering block level code
    ///
    /// - [InterpBlockState::ReadyForNewBlock] -> [InterpBlockState::BuildingCode]
    /// - All others invalid
    StartBlockLevelCode(ParseSpan, usize),

    /// On executing code that returned None, transition back to waiting for a paragraph
    ///
    /// - [InterpBlockState::BuildingCode] -> [InterpBlockState::ReadyForNewBlock]
    EmitNone,

    /// On finishing block-level code, if it intends to own a raw scope, start a raw scope.
    ///
    /// - [InterpBlockState::BuildingCode] -> [InterpBlockState::BuildingRawText]
    OpenRawScope(PyTcRef<RawScopeBuilder>, ParseSpan, usize),

    /// Start a paragraph, executing a transition on the paragraph-level state machine
    ///
    /// - [InterpBlockState::ReadyForNewBlock] -> [InterpBlockState::WritingPara]
    /// - [InterpBlockState::BuildingCode] -> [InterpBlockState::WritingPara]
    /// - [InterpBlockState::BuildingRawText] -> [InterpBlockState::WritingPara]
    StartParagraph(InterpParaTransition),

    /// On encountering a paragraph break (a blank line), add the paragraph to the document
    ///
    /// - [InterpBlockState::WritingPara] -> [InterpBlockState::ReadyForNewBlock]
    EndParagraph,

    /// On encountering a block scope close inside a paragraph,
    /// add the paragraph and close the topmost scope
    ///
    /// - [InterpBlockState::WritingPara] -> [InterpBlockState::ReadyForNewBlock]
    EndParagraphAndCloseManualBlockScope(ParseSpan),

    /// On encountering a DocSegment emitted by code at the very start of a paragraph

    /// On encountering a block scope owner (i.e. a BlockScopeOpen optionally preceded by a Python scope owner),
    /// push a block scope onto the current stack
    ///
    /// - [InterpBlockState::ReadyForNewBlock] -> [InterpBlockState::ReadyForNewBlock]
    /// - [InterpBlockState::BuildingCode] -> [InterpBlockState::ReadyForNewBlock]
    OpenManualBlockScope(Option<(PyTcRef<BlockScopeBuilder>, ParseSpan)>, ParseSpan),

    /// If an eval-bracket emits a Block directly, or a raw scope owner takes raw text and produces Block, push it onto the stack
    ///
    /// - [InterpBlockState::BuildingCode] -> [InterpBlockState::ReadyForNewBlock]
    /// - [InterpBlockState::BuildingRawText] -> [InterpBlockState::ReadyForNewBlock]
    PushBlock(ParseSpan, PyTcRef<Block>),

    /// If an eval-bracket emits a DocSegmentHeader directly, push it onto the segment stack
    ///
    /// - [InterpBlockState::BuildingCode] -> [InterpBlockState::ReadyForNewBlock]
    PushDocSegmentHeader(ParseSpan, PyTcRef<DocSegmentHeader>),

    /// If an eval-bracket emits an TurnipTextSource directly, jump out to the parser and start parsing it
    ///
    /// - [InterpBlockState::BuildingCode] -> (early out)
    EmitNewFile(ParseSpan, TurnipTextSource),

    /// On encountering a scope close, pop the current block from the scope.
    /// May trigger a BlockScopeBuilder, which may then emit a block or a DocSegment directly into the tree.
    ///
    /// - [InterpBlockState::ReadyForNewBlock] -> [InterpBlockState::ReadyForNewBlock]
    CloseManualBlockScope(ParseSpan),
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
    Text(String),
    Raw(String),
    PythonObject(PyTcRef<Inline>),
}
impl InlineNodeToCreate {
    fn to_py_intern(self, py: Python) -> PyResult<PyTcRef<Inline>> {
        match self {
            InlineNodeToCreate::Text(s) => {
                let unescaped_text = Py::new(py, Text::new_rs(py, s.as_str()))?;
                Ok(PyTcRef::of_unchecked(unescaped_text.as_ref(py)))
            }
            InlineNodeToCreate::Raw(raw) => {
                let raw_text = Py::new(py, Raw::new_rs(py, raw.as_str()))?;
                Ok(PyTcRef::of_unchecked(raw_text.as_ref(py)))
            }
            InlineNodeToCreate::PythonObject(obj) => Ok(obj),
        }
    }
    pub(crate) fn to_py(self, py: Python) -> TurnipTextContextlessResult<PyTcRef<Inline>> {
        self.to_py_intern(py).err_as_internal(py)
    }
}

/// Enumeration of all possible interpreter errors
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InterpError {
    #[error("Code close encountered outside of code mode")]
    CodeCloseOutsideCode(ParseSpan),
    // TODO clarify the message that there is no matching scope open *within the current file*
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
    #[error("A Python `TurnipTextSource` was returned by code inside a paragraph")]
    InsertedFileMidPara { code_span: ParseSpan },
    #[error("A Python `DocSegment` was returned by code inside a paragraph")]
    DocSegmentHeaderMidPara { code_span: ParseSpan },
    #[error("A Python `DocSegmentHeader` was built by code inside a block scope")]
    DocSegmentHeaderMidScope {
        code_span: ParseSpan,
        block_close_span: Option<ParseSpan>,
        enclosing_scope_start: ParseSpan,
    },
    #[error("A Python `Block` was returned by a RawScopeBuilder inside a paragraph")]
    BlockCodeFromRawScopeMidPara { code_span: ParseSpan },
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
    PythonErr {
        ctx: String,
        pyerr: String,
        code_span: ParseSpan,
    },
    #[error("Escaped newline (used for sentence continuation) found outside paragraph")]
    EscapedNewlineOutsideParagraph { newline: ParseSpan },
    // This was planned but with the way the new parser is built we can't be anal about newlines inside subfiles without having subfile-newlines affect the correctness of the surrounding file.
    // For now just let unintuitive block syntax through.
    // #[error("Insufficient separation between blocks")]
    // InsufficientBlockSeparation { block_start: ParseSpan },
}

trait MapContextlessResult<T> {
    fn err_as_interp(
        self,
        py: Python,
        ctx: &'static str,
        code_span: ParseSpan,
    ) -> TurnipTextContextlessResult<T>;
    fn err_as_internal(self, py: Python) -> TurnipTextContextlessResult<T>;
}
impl<T> MapContextlessResult<T> for PyResult<T> {
    fn err_as_interp(
        self,
        py: Python,
        ctx: &'static str,
        code_span: ParseSpan,
    ) -> TurnipTextContextlessResult<T> {
        self.map_err(|pyerr| {
            InterpError::PythonErr {
                ctx: ctx.into(),
                pyerr: stringify_pyerr(py, &pyerr),
                code_span,
            }
            .into()
        })
    }
    fn err_as_internal(self, py: Python) -> TurnipTextContextlessResult<T> {
        self.map_err(|pyerr| {
            TurnipTextContextlessError::InternalPython(stringify_pyerr(py, &pyerr))
        })
    }
}

pub enum InterpreterFileAction {
    FileInserted { name: String, contents: String },
    FileEnded,
}

impl Interpreter {
    pub fn push_subfile(&mut self) {
        self.structure.block_stacks.push(vec![])
    }

    pub fn pop_subfile(
        &mut self,
        py: Python,
        py_env: &'_ PyDict,
    ) -> TurnipTextContextlessResult<()> {
        // If we finish while we're parsing a paragraph, jump out of it with a state machine transition
        let transitions = match &mut self.block_state {
            InterpBlockState::ReadyForNewBlock => (None, None),
            InterpBlockState::WritingPara(state) => state.finalize(py)?,
            InterpBlockState::BuildingCode { code_start, .. } => {
                return Err(InterpError::EndedInsideCode {
                    code_start: *code_start,
                }
                .into())
            }
            InterpBlockState::BuildingRawText { builder_span, .. } => {
                return Err(InterpError::EndedInsideRawScope {
                    raw_scope_start: *builder_span, // TODO This is technically wrong but it gets the point across. Should return the raw_scope token start, not the builder start
                }
                .into());
            }
        };

        self.handle_transition(py, py_env, transitions)?;

        self.structure.check_can_end_subfile()?;

        self.structure.block_stacks.pop();

        Ok(())
    }

    pub fn finalize<'a>(
        mut self,
        py: Python,
        py_env: &PyDict,
    ) -> TurnipTextContextlessResult<Py<DocSegment>> {
        self.structure.pop_segments_until_less_than(py, i64::MIN)?;
        self.structure.finalize(py).err_as_internal(py)
    }

    /// Return (block transition, special transition) to be executed in the order (block transition, special transition)
    fn mutate_and_find_transitions(
        &mut self,
        py: Python,
        py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<(
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
                        return Err(
                            InterpError::EscapedNewlineOutsideParagraph { newline: span }.into(),
                        )
                    }

                    CodeOpen(span, n_hashes) => (Some(StartBlockLevelCode(span, n_hashes)), None),

                    // PushBlock with no code managing it
                    BlockScopeOpen(span) => (Some(OpenManualBlockScope(None, span)), None),

                    // PushInlineScope with no code managing it
                    InlineScopeOpen(span) => (
                        Some(StartParagraph(InterpParaTransition::PushInlineScope(
                            None, span,
                        ))),
                        None,
                    ),

                    // StartRawBlock
                    // If we start a raw scope directly, with no owner, go into Inline mode.
                    // Raw text can only be inline content when inserted directly.
                    RawScopeOpen(span, n_hashes) => (
                        Some(StartParagraph(InterpParaTransition::StartRawScope(
                            None, span, n_hashes,
                        ))),
                        None,
                    ),
                    RawScopeClose(span, _) => {
                        return Err(InterpError::RawScopeCloseOutsideRawScope(span).into())
                    }

                    // Try a scope close
                    ScopeClose(span) => match self.structure.get_enclosing_block() {
                        Some(_) => (Some(CloseManualBlockScope(span)), None),
                        None => return Err(InterpError::ScopeCloseOutsideScope(span).into()),
                    },

                    // Complain - not in code mode
                    CodeClose(span, _) => {
                        return Err(InterpError::CodeCloseOutsideCode(span).into())
                    }

                    // Do nothing - we're still ready to receive a new block
                    Newline(_) => (None, None),
                    // Ignore whitespace at the start of a paragraph
                    Whitespace(_) => (None, None),

                    // Enter comment mode
                    Hashes(span, _) => (None, Some(InterpSpecialTransition::StartComment(span))),

                    // Normal text - start a new paragraph
                    _ => (
                        Some(StartParagraph(InterpParaTransition::StartText(
                            tok.stringify_escaped(data).into(),
                        ))),
                        None,
                    ),
                }
            }
            InterpBlockState::WritingPara(state) => state.handle_token(py, py_env, tok, data)?,
            InterpBlockState::BuildingCode {
                code,
                code_start,
                expected_close_len,
            } => {
                match eval_brackets(data, tok, code, code_start, *expected_close_len, py, py_env)? {
                    Some((res, code_span)) => {
                        use EvalBracketResult::*;
                        let block_transition = match res {
                            NeededBlockBuilder(b) => {
                                OpenManualBlockScope(Some((b, code_span)), tok.token_span())
                            }
                            NeededInlineBuilder(i) => StartParagraph(
                                InterpParaTransition::PushInlineScope(Some(i), code_span),
                            ),
                            NeededRawBuilder(r, n_hashes) => OpenRawScope(r, code_span, n_hashes),
                            DocSegmentHeader(s) => PushDocSegmentHeader(code_span, s),
                            // TODO this allows `[something_emitting_block] and a directly following paragraph`.
                            Block(b) => PushBlock(code_span, b),
                            Inline(i) => StartParagraph(InterpParaTransition::PushInlineContent(
                                InlineNodeToCreate::PythonObject(i),
                            )),
                            // TODO this allows `[something_emitting_insertedfile] and a directly following paragraph].`
                            TurnipTextSource(file) => EmitNewFile(code_span, file),
                            PyNone => EmitNone,
                        };
                        (Some(block_transition), None)
                    }
                    None => (None, None),
                }
            }
            InterpBlockState::BuildingRawText {
                builder,
                text,
                builder_span,
                expected_n_hashes,
            } => match tok {
                RawScopeClose(_, n_hashes) if n_hashes == *expected_n_hashes => {
                    // Make sure the RawScopeBuilder produces something that's either Inline or Block
                    let to_emit = RawScopeBuilder::call_build_from_raw(py, builder, text)
                        .err_as_interp(
                            py,
                            "Error while calling .build_from_raw() on object",
                            *builder_span,
                        )?;

                    match to_emit {
                        PyTcUnionRef::A(inl) => (
                            Some(StartParagraph(InterpParaTransition::PushInlineContent(
                                InlineNodeToCreate::PythonObject(inl),
                            ))),
                            None,
                        ),
                        PyTcUnionRef::B(block) => {
                            (Some(PushBlock(builder_span.clone(), block)), None)
                        }
                    }
                }
                _ => {
                    text.push_str(tok.stringify_raw(data));
                    (None, None)
                }
            },
        };

        Ok(transition)
    }

    /// May recurse if StartParagraph(transition)
    fn handle_transition(
        &mut self,
        py: Python,
        py_env: &PyDict,
        transitions: (
            Option<InterpBlockTransition>,
            Option<InterpSpecialTransition>,
        ),
    ) -> TurnipTextContextlessResult<Option<TurnipTextSource>> {
        let (block_transition, special_transition) = transitions;

        let mut file_to_emit = None;

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
                    S::ReadyForNewBlock | S::BuildingCode { .. } | S::BuildingRawText { .. },
                    T::StartParagraph(transition),
                ) => {
                    let mut para_state = InterpParaState::new(py).err_as_internal(py)?;
                    let (new_block_transition, new_special_transition) =
                        para_state.handle_transition(py, Some(transition))?;
                    if new_block_transition.is_some() {
                        return Err(TurnipTextContextlessError::Internal(
                            "An inline transition, initiated with the start of a paragraph, tried to initiate another block transition. This is not allowed and should not be possible.".into()
                        ));
                    }
                    self.handle_transition(py, py_env, (None, new_special_transition))?;
                    S::WritingPara(para_state)
                }
                (S::WritingPara(para_state), T::EndParagraph) => {
                    self.structure
                        .push_to_topmost_block(py, para_state.para().as_ref(py))?;
                    S::ReadyForNewBlock
                }
                (
                    S::WritingPara(para_state),
                    T::EndParagraphAndCloseManualBlockScope(scope_close_span),
                ) => {
                    // End paragraph i.e. push paragraph onto topmost block
                    self.structure
                        .push_to_topmost_block(py, para_state.para().as_ref(py))?;
                    // Pop block
                    let popped_scope = self.structure.pop_enclosing_block_within_data();
                    match popped_scope {
                        Some(popped_scope) => {
                            match popped_scope.build_to_block(py, scope_close_span)? {
                                (Some(PyTcUnionRef::A(block)), _) => {
                                    self.structure.push_to_topmost_block(py, block.as_ref(py))?
                                }
                                (Some(PyTcUnionRef::B(subsegment)), code_span) => {
                                    let code_span = code_span.expect(
                                        "DocSegmentHeader can only occur through executed code",
                                    );
                                    self.structure.push_segment_header(
                                        py,
                                        subsegment,
                                        code_span,
                                        Some(scope_close_span),
                                    )?
                                }
                                (None, _) => {}
                            }
                        }
                        None => {
                            return Err(InterpError::ScopeCloseOutsideScope(scope_close_span).into())
                        }
                    }
                    S::ReadyForNewBlock
                }

                (
                    S::BuildingCode { .. } | S::BuildingRawText { .. },
                    T::PushBlock(_code_span, b),
                ) => {
                    self.structure.push_to_topmost_block(py, b.as_ref(py))?;
                    S::ReadyForNewBlock
                }

                (S::BuildingCode { .. }, T::PushDocSegmentHeader(code_span, segment)) => {
                    self.structure
                        .push_segment_header(py, segment, code_span.clone(), None)?;
                    S::ReadyForNewBlock
                }

                (S::BuildingCode { .. }, T::EmitNone) => S::ReadyForNewBlock,

                (S::BuildingCode { .. }, T::OpenRawScope(r, builder_span, expected_n_hashes)) => {
                    S::BuildingRawText {
                        builder: r,
                        text: "".into(),
                        builder_span,
                        expected_n_hashes,
                    }
                }
                (
                    S::ReadyForNewBlock | S::BuildingCode { .. },
                    T::OpenManualBlockScope(builder, scope_start),
                ) => {
                    self.structure
                        .push_enclosing_block_within_data(InterpManualBlockScopeState {
                            builder,
                            children: Py::new(py, BlockScope::new_empty(py)).err_as_internal(py)?,
                            scope_start,
                        });
                    S::ReadyForNewBlock
                }
                (S::ReadyForNewBlock, T::CloseManualBlockScope(scope_close_span)) => {
                    let popped_scope = self.structure.pop_enclosing_block_within_data();
                    match popped_scope {
                        Some(popped_scope) => {
                            match popped_scope.build_to_block(py, scope_close_span)? {
                                (Some(PyTcUnionRef::A(block)), _) => {
                                    self.structure.push_to_topmost_block(py, block.as_ref(py))?
                                }
                                (Some(PyTcUnionRef::B(subsegment)), code_span) => {
                                    let code_span = code_span.expect(
                                        "DocSegmentHeader can only occur through executed code",
                                    );
                                    self.structure.push_segment_header(
                                        py,
                                        subsegment,
                                        code_span,
                                        Some(scope_close_span),
                                    )?
                                }
                                (None, _) => {}
                            }
                        }
                        None => {
                            return Err(InterpError::ScopeCloseOutsideScope(scope_close_span).into())
                        }
                    }
                    S::ReadyForNewBlock
                }
                (
                    S::ReadyForNewBlock | S::BuildingCode { .. },
                    T::EmitNewFile(_emitter_span, file),
                ) => {
                    file_to_emit = Some(file);
                    // Discard the BuildingCode state so that it doesn't continue into the subfile
                    S::ReadyForNewBlock
                }
                (_, transition) => {
                    return Err(TurnipTextContextlessError::Internal(format!(
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
                    return Err(TurnipTextContextlessError::Internal(format!(
                        "Invalid special state/transition pair encountered ({0:?}, {1:?})",
                        self.comment_state, transition
                    )))
                }
            }
        }

        Ok(file_to_emit)
    }
}
