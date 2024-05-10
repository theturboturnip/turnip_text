use pyo3::prelude::*;

use crate::{
    interpreter::{
        error::{syntax::TTSyntaxError, HandleInternalPyErr, TTResult},
        lexer::TTToken,
        state_machines::{py_internal_alloc, BlockElem, ProcStatus},
    },
    python::{
        interop::{BlockScope, Document, Header},
        typeclass::PyTcRef,
    },
    util::ParseContext,
};

use super::{BlockLevelProcessor, BlockMode};

/// At the top level of the document, headers are allowed and manipulate the InterimDocumentStructure.
pub struct TopLevelBlockMode {
    structure: InterimDocument,
}
impl BlockLevelProcessor<TopLevelBlockMode> {
    pub fn new(py: Python) -> TTResult<Self> {
        Ok(Self {
            inner: TopLevelBlockMode {
                structure: InterimDocument::new(py)?,
            },
            expects_n_blank_lines_after: None,
        })
    }
    pub fn finalize(self) -> Py<Document> {
        self.inner.structure.finalize()
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
    document: Py<Document>,
    topmost_block: Py<BlockScope>,
}
impl InterimDocument {
    pub fn new(py: Python) -> TTResult<Self> {
        let document = Document::empty(py).expect_pyok("Allocating Document");
        let topmost_block = document.contents.clone_ref(py);
        let document = py_internal_alloc(py, document)?;
        Ok(Self {
            document,
            topmost_block,
        })
    }

    pub fn finalize(self) -> Py<Document> {
        self.document
    }

    fn push_segment_header(&mut self, py: Python, header: PyTcRef<Header>) -> TTResult<()> {
        let bound_document = self.document.bind(py).borrow();
        let docsegment = bound_document
            .append_header(py, header.bind(py))
            .expect_pyok("push_segment_header");
        self.topmost_block = docsegment.borrow().contents.clone_ref(py);
        Ok(())
    }

    fn push_to_topmost_block(&self, py: Python, block: &Bound<'_, PyAny>) -> TTResult<()> {
        Ok(self
            .topmost_block
            .borrow_mut(py)
            .append_block(block)
            .expect_pyok("BlockScope::append_block with presumed Block"))
    }
}
