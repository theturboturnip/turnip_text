use pyo3::prelude::*;

use crate::{
    interpreter::{
        error::{syntax::TTSyntaxError, HandleInternalPyErr, TTResult},
        lexer::TTToken,
        state_machines::{py_internal_alloc, BlockElem, ProcStatus},
    },
    python::{
        interop::{Blocks, Header},
        typeclass::PyTcRef,
    },
    util::{ParseContext, ParseSpan},
};

use super::{BlockLevelProcessor, BlockMode};

pub struct BlockScopeBlockMode {
    ctx: ParseContext,
    blocks: Py<Blocks>,
}
impl BlockMode for BlockScopeBlockMode {
    fn on_close_scope(&mut self, py: Python, tok: TTToken, _data: &str) -> TTResult<ProcStatus> {
        // This builder may receive tokens from inner files.
        // If it receives a token from an inner file, it returns an error.
        // This fulfils the contract for [TokenProcessor::process_token].
        if !self.ctx.try_extend(&tok.token_span()) {
            // Closing block scope from different file
            // This must be a block-level scope close, because if an unbalanced scope close appeared in inline mode it would already have errored and not bubbled out.
            Err(TTSyntaxError::BlockScopeCloseOutsideScope(tok.token_span()).into())
        } else {
            Ok(ProcStatus::Pop(Some((
                self.ctx,
                BlockElem::Blocks(self.blocks.clone_ref(py)).into(),
            ))))
        }
    }

    fn on_eof(&mut self, _py: Python, tok: TTToken) -> TTResult<ProcStatus> {
        Err(TTSyntaxError::EndedInsideScope {
            scope_start: self.ctx.first_tok(),
            eof_span: tok.token_span(),
        }
        .into())
    }

    fn on_header(&mut self, py: Python, header: PyTcRef<Header>) -> TTResult<()> {
        self.blocks
            .borrow_mut(py)
            .append_block(header.bind(py))
            .expect_pyok("Blocks::append_block with Header");
        Ok(())
    }

    fn on_block(&mut self, py: Python, block: &Bound<'_, PyAny>) -> TTResult<()> {
        self.blocks
            .borrow_mut(py)
            .append_block(block)
            .expect_pyok("Blocks::append_block with BlockElem");
        Ok(())
    }
}
impl BlockLevelProcessor<BlockScopeBlockMode> {
    pub fn new(py: Python, first_tok: ParseSpan, last_tok: ParseSpan) -> TTResult<Self> {
        Ok(Self {
            inner: BlockScopeBlockMode {
                ctx: ParseContext::new(first_tok, last_tok),
                blocks: py_internal_alloc(py, Blocks::new_empty(py))?,
            },
            expects_n_blank_lines_after: None,
        })
    }
}
