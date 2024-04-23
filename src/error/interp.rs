use pyo3::{PyResult, Python};
use thiserror::Error;

use crate::{
    python::{
        interop::{Block, DocSegmentHeader},
        typeclass::PyTcRef,
    },
    util::{ParseContext, ParseSpan},
};

use super::{stringify_pyerr, TurnipTextContextlessError, TurnipTextContextlessResult};

#[derive(Debug, Clone)]
pub enum InlineModeContext {
    Paragraph(ParseContext),
    InlineScope { scope_start: ParseSpan },
}

/// Sufficient context or scope for error messages.
/// Small things (code and eval-brackets that return results) are given as ParseSpans,
/// as we assume it's reasonable to print them in the error traceback.
/// Big things (paragraphs and block scopes) are given as ParseContexts -
/// they might be big, so we should provide separate snippets for the start and end.
#[derive(Debug, Clone)]
pub enum BlockModeElem {
    HeaderFromCode(ParseSpan),
    Para(ParseContext),
    /// A complete block scope
    BlockScope(ParseContext),
    BlockFromCode(ParseSpan),
    SourceFromCode(ParseSpan),
    AnyToken(ParseSpan),
}

/// Enumeration of all possible interpreter errors
#[derive(Debug, Clone, Error)]
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
    EndedInsideCode {
        code_start: ParseSpan,
        eof_span: ParseSpan,
    },
    #[error("File ended inside raw scope")]
    EndedInsideRawScope {
        raw_scope_start: ParseSpan,
        eof_span: ParseSpan,
    },
    #[error("File ended inside scope")]
    EndedInsideScope {
        scope_start: ParseSpan,
        eof_span: ParseSpan,
    },

    #[error("Encountered a block-scope-open in inline mode")]
    BlockScopeOpenedInInlineMode {
        inl_mode: InlineModeContext,
        block_scope_open: ParseSpan,
    },
    #[error("Code emitted a Python `Block` in inline mode")]
    CodeEmittedBlockInInlineMode {
        inl_mode: InlineModeContext,
        block: PyTcRef<Block>,
        code_span: ParseSpan,
    },
    #[error("Code emitted a Python `Header` in inline mode")]
    CodeEmittedHeaderInInlineMode {
        inl_mode: InlineModeContext,
        header: PyTcRef<DocSegmentHeader>,
        code_span: ParseSpan,
    },
    #[error("Code emitted a Header inside a block scope")]
    CodeEmittedHeaderInBlockScope {
        block_scope_start: ParseSpan,
        header: PyTcRef<DocSegmentHeader>,
        code_span: ParseSpan,
    },
    #[error("Code emitted a Python `TurnipTextSource` in inline mode")]
    CodeEmittedSourceInInlineMode {
        inl_mode: InlineModeContext,
        code_span: ParseSpan,
    },

    #[error("Encountered a sentence break inside an inline scope")]
    SentenceBreakInInlineScope {
        scope_start: ParseSpan,
        sentence_break: ParseSpan,
    },
    #[error("Escaped newline (used for sentence continuation) found outside paragraph")]
    EscapedNewlineOutsideParagraph { newline: ParseSpan },
    #[error("Insufficient separation between blocks")]
    InsufficientBlockSeparation {
        last_block: BlockModeElem,
        next_block_start: BlockModeElem,
    },
}

pub trait MapContextlessResult<T> {
    fn err_as_internal(self, py: Python) -> TurnipTextContextlessResult<T>;
}
impl<T> MapContextlessResult<T> for PyResult<T> {
    fn err_as_internal(self, py: Python) -> TurnipTextContextlessResult<T> {
        self.map_err(|pyerr| {
            TurnipTextContextlessError::InternalPython(stringify_pyerr(py, &pyerr))
        })
    }
}
