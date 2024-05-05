use pyo3::prelude::*;

use crate::{
    interpreter::{
        error::{syntax::TTSyntaxError, TTResult},
        lexer::TTToken,
        state_machines::{BlockElem, ProcStatus},
    },
    python::{
        interop::{BlockScope, DocSegment, Document, Header},
        typeclass::{PyInstanceList, PyTcRef},
    },
    util::ParseContext,
};

use super::{BlockLevelProcessor, BlockMode};

/// At the top level of the document, headers are allowed and manipulate the InterimDocumentStructure.
pub struct TopLevelBlockMode {
    structure: InterimDocument,
}
impl BlockLevelProcessor<TopLevelBlockMode> {
    pub fn new(py: Python) -> PyResult<Self> {
        Ok(Self {
            inner: TopLevelBlockMode {
                structure: InterimDocument::new(py)?,
            },
            expects_n_blank_lines_after: None,
        })
    }
    pub fn finalize(mut self, py: Python<'_>) -> TTResult<Py<Document>> {
        self.inner
            .structure
            .pop_segments_until_less_than(py, i64::MIN)?;
        Ok(self.inner.structure.finalize(py)?)
    }
}
impl BlockMode for TopLevelBlockMode {
    fn on_close_scope(&mut self, _py: Python, tok: TTToken, _data: &str) -> TTResult<ProcStatus> {
        // This builder may receive tokens from inner files.
        // It always returns an error.
        // This fulfils the contract for [TokenProcessor::process_token].
        Err(TTSyntaxError::BlockScopeCloseOutsideScope(tok.token_span()).into())
    }

    // When EOF comes, we don't produce anything to bubble up - there's nothing above us!
    fn on_eof(&mut self, _py: Python, _tok: TTToken) -> TTResult<ProcStatus> {
        // This is the only exception to the contract for [TokenProcessor::process_token].
        // There is never a builder above this one, so there is nothing that can reprocess the token.
        Ok(ProcStatus::Continue)
    }

    fn on_header(
        &mut self,
        py: Python,
        header: PyTcRef<Header>,
        _header_ctx: ParseContext,
    ) -> TTResult<ProcStatus> {
        self.structure.push_segment_header(py, header)?;
        Ok(ProcStatus::Continue)
    }

    fn on_block(
        &mut self,
        py: Python,
        block: BlockElem,
        _block_ctx: ParseContext,
    ) -> TTResult<ProcStatus> {
        self.structure.push_to_topmost_block(py, block.bind(py))?;
        Ok(ProcStatus::Continue)
    }
}

/// Provides a simple interface to a [Document] in-progress
/// FUTURE could merge into the TopLevelBlock mode but that might make code less clean
pub struct InterimDocument {
    /// Top level content of the document
    /// All text leading up to the first DocSegment
    toplevel_content: Py<BlockScope>,
    /// The top level segments
    toplevel_segments: PyInstanceList<DocSegment>,
    /// The stack of DocSegments leading up to the current doc segment.
    segment_stack: Vec<InterimDocSegment>,
}
impl InterimDocument {
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
        let subsegment = InterimDocSegment::new(py, header, subsegment_weight)?;
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
struct InterimDocSegment {
    header: PyTcRef<Header>,
    weight: i64, // cached from `header.weight` in Python
    content: Py<BlockScope>,
    subsegments: PyInstanceList<DocSegment>,
}
impl InterimDocSegment {
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
