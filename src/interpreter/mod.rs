use pyo3::prelude::*;
use pyo3::{types::PyDict, Py, Python};
use std::fmt::Debug;
use thiserror::Error;

use crate::{
    error::{
        stringify_pyerr, TurnipTextContextlessError, TurnipTextContextlessResult, TurnipTextError,
        TurnipTextResult,
    },
    lexer::{lex, LexedStrIterator},
    util::ParseSpan,
};

pub mod python;
use python::{
    interop::*,
    typeclass::{PyInstanceList, PyTcRef},
};

pub mod next;

pub struct ParsingFile {
    name: String,
    contents: String,
    token_stream: LexedStrIterator,
}
impl Debug for ParsingFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsingFile")
            .field("name", &self.name)
            .field("contents", &self.contents)
            .field("token_stream", &"...".to_string())
            .finish()
    }
}
impl ParsingFile {
    pub fn new(file_idx: usize, name: String, contents: String) -> Self {
        Self {
            name,
            token_stream: lex(file_idx, &contents),
            contents,
        }
    }

    pub fn name<'a>(&'a self) -> &'a str {
        &self.name
    }

    pub fn contents<'a>(&'a self) -> &'a str {
        &self.contents
    }
}

pub struct TurnipTextParser {
    // The stack of currently parsed file (spawned from, indices). `spawned from` is None for the first file, Some for all others
    file_stack: Vec<(Option<ParseSpan>, usize)>,
    files: Vec<ParsingFile>,
    interp: crate::interpreter::next::Interpreter,
}
impl TurnipTextParser {
    pub fn new(py: Python, file_name: String, file_contents: String) -> TurnipTextResult<Self> {
        let file = ParsingFile::new(0, file_name, file_contents);
        let files = vec![file];
        let interp = crate::interpreter::next::Interpreter::new(py)
            .map_err(|pyerr| TurnipTextError::InternalPython(stringify_pyerr(py, &pyerr)))?;
        Ok(Self {
            file_stack: vec![(None, 0)],
            files,
            interp,
        })
    }
    pub fn parse(mut self, py: Python, py_env: &PyDict) -> TurnipTextResult<Py<DocSegment>> {
        // Call handle_tokens until it breaks out returning FileInserted or FileEnded.
        // FileEnded will be returned exactly once more than FileInserted - FileInserted is only returned for subfiles, FileEnded is returned for all subfiles AND the initial file.
        // We handle this because the file stack, Vec<ParsingFile>, and interpreter each have one file's worth of content pushed in initially.
        loop {
            let action = {
                let file_idx = match self.file_stack.last_mut() {
                    None => break,
                    Some((_, file_idx)) => file_idx,
                };
                let file = &mut self.files[*file_idx];
                self.interp.handle_tokens(
                    py,
                    py_env,
                    &mut file.token_stream,
                    *file_idx,
                    &file.contents,
                )
            };
            let action = match action {
                Ok(action) => action,
                Err(err) => return Err((self.files, err).into()),
            };
            match action {
                InterpreterFileAction::FileInserted {
                    emitted_by,
                    name,
                    contents,
                } => {
                    let file_idx = self.files.len();
                    self.files.push(ParsingFile::new(file_idx, name, contents));
                    self.file_stack.push((Some(emitted_by), file_idx));
                    self.interp.push_subfile();
                }
                InterpreterFileAction::FileEnded => {
                    let (emitted_by, _) = self
                        .file_stack
                        .pop()
                        .expect("We just handled tokens from a file, there must be one");
                    match self.interp.pop_subfile(py, py_env, emitted_by) {
                        Ok(()) => {}
                        Err(err) => return Err((self.files, err).into()),
                    };
                }
            };
        }

        self.interp
            .finalize(py, py_env)
            .map_err(|err| (self.files, err).into())
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
}
impl InterimDocumentStructure {
    pub fn new(py: Python) -> PyResult<Self> {
        Ok(Self {
            toplevel_content: Py::new(py, BlockScope::new_empty(py))?,
            toplevel_segments: PyInstanceList::new(py),
            segment_stack: vec![],
        })
    }

    pub fn finalize(self, py: Python) -> PyResult<Py<DocSegment>> {
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

    fn push_segment_header(
        &mut self,
        py: Python,
        header: PyTcRef<DocSegmentHeader>,
    ) -> TurnipTextContextlessResult<()> {
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
        // Figure out which block is actually the topmost block.
        // If the block stack has elements, add it to the topmost element.
        // If the block stack is empty, and there are elements on the DocSegment stack, take the topmost element of the DocSegment stack
        // If the block stack is empty and the segment stack is empty add to toplevel content.
        let child_list_ref = match self.segment_stack.last() {
            Some(segment) => &segment.content,
            None => &self.toplevel_content,
        };
        child_list_ref
            .borrow_mut(py)
            .push_block(block)
            .err_as_internal(py)
    }
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

/// Enumeration of all possible interpreter errors
///
/// TODO in all cases except XCloseOutsideY and EndedInsideX each of these should have two ParseSpans - the offending item, and the context for why it's offending.
/// e.g. SentenceBreakInInlineScope should point to both the start of the inline scope *and* the sentence break! and probably any escaped newlines inbetween as well!
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InterpError {
    #[error("Code close encountered outside of code mode")]
    CodeCloseOutsideCode(ParseSpan),
    #[error("Scope close encountered in block mode when this file had no open block scopes")]
    BlockScopeCloseOutsideScope(ParseSpan),
    #[error("Scope close encountered in inline mode when there were no open inline scopes")]
    InlineScopeCloseOutsideScope(ParseSpan),
    #[error("Raw scope close when not in a raw scope")]
    RawScopeCloseOutsideRawScope(ParseSpan),
    #[error("File ended inside code block")]
    EndedInsideCode { code_start: ParseSpan },
    #[error("File ended inside raw scope")]
    EndedInsideRawScope { raw_scope_start: ParseSpan },
    #[error("File ended inside scope")]
    EndedInsideScope { scope_start: ParseSpan },
    #[error("Block scope open encountered in inline mode")]
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
    #[error("Insufficient separation between blocks")]
    InsufficientBlockSeparation {
        last_block: ParseSpan,
        next_block_start: ParseSpan,
    },
    #[error(
        "Insufficient separation between the end of a paragraph and the start of a new block/file"
    )]
    InsufficientParaNewBlockOrFileSeparation {
        para: ParseSpan,
        next_block_start: ParseSpan,
        was_block_not_file: bool,
    },
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
    FileInserted {
        emitted_by: ParseSpan,
        name: String,
        contents: String,
    },
    FileEnded,
}
