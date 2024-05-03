use pyo3::prelude::*;
use pyo3::{types::PyDict, Py, Python};
use std::fmt::Debug;

use crate::python::interop::{BlockScope, DocSegment, Document, Header};
use crate::python::typeclass::{PyInstanceList, PyTcRef};
use crate::{
    error::{TTErrorWithContext, TTResult, TTResultWithContext},
    lexer::{lex, LexedStrIterator},
    util::ParseSpan,
};

use self::state_machines::ProcessorStacks;

mod state_machines;

pub type ParserEnv<'a> = &'a Bound<'a, PyDict>;

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
    pub fn new(file_idx: usize, name: String, contents: String) -> TTResultWithContext<Self> {
        // Can't use map_err here because the closure can't move out of name if we use name later
        let token_stream = match lex(file_idx, &contents) {
            Ok(ts) => ts,
            Err(_) => return Err(TTErrorWithContext::NullByteFoundInSource { source_name: name }),
        };
        Ok(Self {
            name,
            contents,
            token_stream,
        })
    }

    pub fn name<'a>(&'a self) -> &'a str {
        &self.name
    }

    pub fn contents<'a>(&'a self) -> &'a str {
        &self.contents
    }
}

pub enum FileEvent {
    FileInserted {
        emitted_by_code: ParseSpan,
        name: String,
        contents: String,
    },
    FileEnded,
}

pub struct TurnipTextParser {
    // The stack of currently parsed file (spawned from, indices). `spawned from` is None for the first file, Some for all others
    file_stack: Vec<(Option<ParseSpan>, usize)>,
    files: Vec<ParsingFile>,
    builders: ProcessorStacks,
}
impl TurnipTextParser {
    pub fn new(py: Python, file_name: String, file_contents: String) -> TTResultWithContext<Self> {
        let file = ParsingFile::new(0, file_name, file_contents)?;
        let files = vec![file];
        let builders = ProcessorStacks::new(py)?;
        Ok(Self {
            file_stack: vec![(None, 0)],
            files,
            builders,
        })
    }
    pub fn parse(mut self, py: Python, py_env: ParserEnv) -> TTResultWithContext<Py<Document>> {
        // Call process_tokens until it breaks out returning FileInserted or FileEnded.
        // FileEnded will be returned exactly once more than FileInserted - FileInserted is only returned for subfiles, FileEnded is returned for all subfiles AND the initial file.
        // We handle this because the file stack, Vec<ParsingFile>, and interpreter each have one file's worth of content pushed in initially.
        loop {
            let action = {
                let file_idx = match self.file_stack.last_mut() {
                    None => break,
                    Some((_, file_idx)) => file_idx,
                };
                let file = &mut self.files[*file_idx];
                self.builders.top_stack().process_tokens(
                    py,
                    py_env,
                    &mut file.token_stream,
                    &file.contents,
                )
            };
            let action = match action {
                Ok(action) => action,
                Err(err) => return Err((self.files, err).into()),
            };
            match action {
                FileEvent::FileInserted {
                    emitted_by_code,
                    name,
                    contents,
                } => {
                    let file_idx = self.files.len();
                    self.files.push(ParsingFile::new(file_idx, name, contents)?);
                    self.file_stack.push((Some(emitted_by_code), file_idx));
                    self.builders.push_subfile();
                }
                FileEvent::FileEnded => {
                    let (emitted_by, _) = self
                        .file_stack
                        .pop()
                        .expect("We just handled tokens from a file, there must be one");
                    self.builders.pop_subfile(emitted_by)
                }
            };
        }

        self.builders
            .finalize(py)
            .map_err(|err| (self.files, err).into())
    }
}

// TODO move into state_machines
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

    pub fn finalize(self, py: Python) -> PyResult<Py<Document>> {
        assert_eq!(
            self.segment_stack.len(),
            0,
            "Tried to finalize the document with in-progress segments?"
        );
        Py::new(
            py,
            Document::new_rs(
                self.toplevel_content.clone(),
                self.toplevel_segments.clone(),
            ),
        )
    }

    fn push_segment_header(&mut self, py: Python, header: PyTcRef<Header>) -> TTResult<()> {
        let subsegment_weight = Header::get_weight(py, header.bind(py))?;

        // If there are items in the segment_stack, pop from self.segment_stack until the toplevel weight < subsegment_weight
        self.pop_segments_until_less_than(py, subsegment_weight)?;

        // We know the thing at the top of the segment stack has a weight < subsegment_weight
        // Push pending segment state to the stack
        let subsegment = InterpDocSegmentState::new(py, header, subsegment_weight)?;
        self.segment_stack.push(subsegment);

        Ok(())
    }

    /// Pop segments from the segment stack until either
    /// - there are no segments left
    /// - self.segment_stack.last() is Some() and has a weight < the target weight.
    fn pop_segments_until_less_than(&mut self, py: Python, weight: i64) -> TTResult<()> {
        let mut curr_toplevel_weight = match self.segment_stack.last() {
            Some(segment) => segment.weight,
            None => return Ok(()),
        };

        // We only get to this point if self.segment_stack.last() == Some
        while curr_toplevel_weight >= weight {
            // We know self.segment_stack.last() exists and has a weight greater than or equal to the target.
            // We have to finish it and push it into the next one.
            let segment_to_finish = self
                .segment_stack
                .pop()
                .expect("Just checked, it isn't empty");

            let segment = segment_to_finish.finalize(py)?;

            // Push the newly finished segment into the next one up,
            // and return the weight of that segment to see if it's greater than or equal to the target.
            curr_toplevel_weight = match self.segment_stack.last() {
                // If there's another segment, push the new finished segment into it,
                // and return that weight
                Some(x) => {
                    x.subsegments.append_checked(segment.bind(py))?;
                    x.weight
                }
                // Otherwise just push it into toplevel_segments.
                // The segment stack is now empty.
                None => {
                    self.toplevel_segments.append_checked(segment.bind(py))?;
                    return Ok(());
                }
            };
        }

        Ok(())
    }

    fn push_to_topmost_block(&self, py: Python, block: &Bound<'_, PyAny>) -> TTResult<()> {
        // Figure out which block is actually the topmost block.
        // If the block stack has elements, add it to the topmost element.
        // If the block stack is empty, and there are elements on the DocSegment stack, take the topmost element of the DocSegment stack
        // If the block stack is empty and the segment stack is empty add to toplevel content.
        let child_list_ref = match self.segment_stack.last() {
            Some(segment) => &segment.content,
            None => &self.toplevel_content,
        };
        Ok(child_list_ref.borrow_mut(py).push_block(block)?)
    }
}

#[derive(Debug)]
struct InterpDocSegmentState {
    header: PyTcRef<Header>,
    weight: i64,
    content: Py<BlockScope>,
    subsegments: PyInstanceList<DocSegment>,
}
impl InterpDocSegmentState {
    fn new(py: Python, header: PyTcRef<Header>, weight: i64) -> PyResult<Self> {
        Ok(Self {
            header,
            weight,
            content: Py::new(py, BlockScope::new_empty(py))?,
            subsegments: PyInstanceList::new(py),
        })
    }
    fn finalize(self, py: Python) -> PyResult<Py<DocSegment>> {
        Py::new(
            py,
            DocSegment::new_checked(py, self.header, self.content, self.subsegments)?,
        )
    }
}
