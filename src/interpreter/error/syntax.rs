use thiserror::Error;

use crate::{
    python::{interop::Block, typeclass::PyTcRef},
    util::{ParseContext, ParseSpan},
};

/// Context for errors that take place because the parser was in inline-mode.
/// Used to indicate why the parser was in inline-mode at that time - either it was inside a Paragraph,
/// or it was inside an Inlines
#[derive(Debug, Clone)]
pub enum InlineModeContext {
    Paragraph(ParseContext),
    Inlines { scope_start: ParseSpan },
}

/// Sufficient context or scope for error messages.
/// Small things (code and eval-brackets that return results) are given as ParseSpans,
/// as we assume it's reasonable to print them in the error traceback.
/// Big things (paragraphs and block scopes) are given as ParseContexts -
/// they might be big, so we should provide separate snippets for the start and end.
#[derive(Debug, Clone)]
pub enum BlockModeElem {
    Para(ParseContext),
    /// A complete block scope
    Blocks(ParseContext),
    BlockFromCode(ParseSpan),
    SourceFromCode(ParseSpan),
    AnyToken(ParseSpan),
}

/// Enumeration of all possible syntax errors
#[derive(Debug, Clone, Error)]
pub enum TTSyntaxError {
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
    EscapedNewlineInBlockMode { newline: ParseSpan },
    #[error("Insufficient separation between blocks")]
    InsufficientBlockSeparation {
        last_block: BlockModeElem,
        next_block_start: BlockModeElem,
    },
}
