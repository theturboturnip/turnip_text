use pyo3::prelude::*;

use crate::{
    interpreter::{
        error::{syntax::TTSyntaxError, TTResult},
        lexer::TTToken,
        state_machines::{py_internal_alloc, BlockElem, ProcStatus},
    },
    python::{
        interop::{BlockScope, Header},
        typeclass::PyTcRef,
    },
    util::{ParseContext, ParseSpan},
};

use super::{BlockLevelProcessor, BlockMode};

pub struct BlockScopeBlockMode {
    ctx: ParseContext,
    block_scope: Py<BlockScope>,
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
                BlockElem::BlockScope(self.block_scope.clone_ref(py)).into(),
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

    fn on_header(
        &mut self,
        _py: Python,
        header: PyTcRef<Header>,
        header_ctx: ParseContext,
    ) -> TTResult<ProcStatus> {
        Err(TTSyntaxError::CodeEmittedHeaderInBlockScope {
            block_scope_start: self.ctx.first_tok(),
            header,
            code_span: header_ctx.full_span(),
        }
        .into())
    }

    fn on_block(
        &mut self,
        py: Python,
        block: BlockElem,
        _block_ctx: ParseContext,
    ) -> TTResult<ProcStatus> {
        self.block_scope.borrow_mut(py).push_block(block.bind(py))?;
        Ok(ProcStatus::Continue)
    }
}
impl BlockLevelProcessor<BlockScopeBlockMode> {
    pub fn new(py: Python, first_tok: ParseSpan, last_tok: ParseSpan) -> TTResult<Self> {
        Ok(Self {
            inner: BlockScopeBlockMode {
                ctx: ParseContext::new(first_tok, last_tok),
                block_scope: py_internal_alloc(py, BlockScope::new_empty(py))?,
            },
            expects_n_blank_lines_after: None,
        })
    }
}
