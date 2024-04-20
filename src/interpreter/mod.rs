use pyo3::prelude::*;
use pyo3::{types::PyDict, Py, Python};
use std::fmt::Debug;
use std::rc::Rc;

use crate::error::interp::MapContextlessResult;
use crate::lexer::{LexError, TTToken};
use crate::python::interop::{BlockScope, DocSegment, DocSegmentHeader, TurnipTextSource};
use crate::python::typeclass::{PyInstanceList, PyTcRef};
use crate::{
    error::{stringify_pyerr, TurnipTextContextlessResult, TurnipTextError, TurnipTextResult},
    lexer::{lex, LexedStrIterator},
    util::ParseSpan,
};

use self::state_machines::BuilderStacks;

mod state_machines;

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
    interp: Interpreter,
}
impl TurnipTextParser {
    pub fn new(py: Python, file_name: String, file_contents: String) -> TurnipTextResult<Self> {
        let file = ParsingFile::new(0, file_name, file_contents);
        let files = vec![file];
        let interp = Interpreter::new(py)
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
                    emitted_by_code,
                    name,
                    contents,
                } => {
                    let file_idx = self.files.len();
                    self.files.push(ParsingFile::new(file_idx, name, contents));
                    self.file_stack.push((Some(emitted_by_code), file_idx));
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

pub enum InterpreterFileAction {
    FileInserted {
        emitted_by_code: ParseSpan,
        name: String,
        contents: String,
    },
    FileEnded,
}

pub struct Interpreter {
    builders: BuilderStacks,
}
impl Interpreter {
    pub fn new(py: Python) -> PyResult<Self> {
        Ok(Self {
            builders: BuilderStacks::new(py)?,
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
            match self
                .builders
                .top_stack()
                .process_token(py, py_env, tok, data)?
            {
                None => continue,
                Some((emitted_by_code, TurnipTextSource { name, contents })) => {
                    return Ok(InterpreterFileAction::FileInserted {
                        emitted_by_code,
                        name,
                        contents,
                    });
                }
            }
        }
        Ok(InterpreterFileAction::FileEnded)
    }

    pub fn push_subfile(&mut self) {
        self.builders.push_subfile()
    }

    pub fn pop_subfile(
        &mut self,
        py: Python,
        py_env: &'_ PyDict,
        subfile_emitted_by: Option<ParseSpan>,
    ) -> TurnipTextContextlessResult<()> {
        let stack = self.builders.pop_subfile(subfile_emitted_by);
        Ok(())
    }

    pub fn finalize(
        self,
        py: Python,
        py_env: &PyDict,
    ) -> TurnipTextContextlessResult<Py<DocSegment>> {
        let rc_refcell_top = self.builders.finalize();
        match Rc::try_unwrap(rc_refcell_top) {
            Err(_) => panic!("Shouldn't have any other stacks holding references to this"),
            Ok(refcell_top) => refcell_top.into_inner().finalize(py),
        }
    }
}
