use pyo3::{PyResult, Python};
use thiserror::Error;

use crate::util::ParseSpan;

use super::{stringify_pyerr, TurnipTextContextlessError, TurnipTextContextlessResult};

/// Enumeration of all possible interpreter errors
///
/// TODO add "in-paragraph" flags to MidPara errors to tell if they're in a paragraph or in a code-owning-inline context
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

pub trait MapContextlessResult<T> {
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
