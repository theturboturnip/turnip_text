//! These modules handle block-mode parsing - both at the top level with [TopLevelProcessor] and inside a block scope with [BlockScopeProcessor].
//! In both cases, all types of Block can be generated.
//! Opening a scope opens an [AmbiguousScopeProcessor], which either emits [BlockElem::Blocks] or [InlineElem::InlineScope].
//! Opening a raw scope opens a [RawStringProcessor].
//! Comments are allowed.
//! Escaped newlines are not allowed.
//! Code is allowed.
//! - Encountered [BlockElem]s go directly into the document,
//!   and put the parser into a state where there must be no content until the next newline.
//!  
//!   This prevents confusing situations where a paragraph could start on the same line as emitting a block,
//!   making that block appear to be attached to the paragraph.
//!   Previously I experimented with requiring a full blank line, but that prevents some clearly visually separated syntax such as
//!   ```text
//!   [item]{
//!     stuff
//!   }
//!   [item]{
//!     more stuff
//!   }
//!   ```
//! - Encountered [InlineElem]s, including Raw from a raw scope, automatically create a new [ParagraphProcessor] which will eventually emit a [BlockElem::Paragraph]
//! - Encountered [Header]s are only allowed in top-level mode, and are pushed to the document to close old segments/create a new one based on its weight.
//!   They also put the parser into a state where there must be no content until the next newline.
//! - Encountered [TurnipTextSource]s are allowed, which means the parser must tolerate receiving tokens from inner files.
//!   In block-scopes, close-block-scope tokens are only accepted if they come from the same file as the Blocks started. If they are received from inner files, those inner files must be unbalanced.
//!   They also put the parser into a state where there must be no content until the next newline.

use pyo3::prelude::*;

use crate::{
    interpreter::{
        error::{
            syntax::{BlockModeElem, TTSyntaxError},
            TTResult,
        },
        lexer::{Escapable, TTToken},
        UserPythonEnv,
    },
    python::{
        interop::{Blocks, Header},
        typeclass::PyTcRef,
    },
    util::{ParseContext, ParseSpan},
};

use super::{
    ambiguous_scope::{AmbiguousScopeProcessor, NoResolveCallbacks},
    code::CodeProcessor,
    comment::CommentProcessor,
    inline::{ParagraphProcessor, RawStringProcessor},
    rc_refcell, BlockElem, DocElement, EmittedElement, ProcStatus, TokenProcessor,
};

mod block_scope;
mod toplevel;

use block_scope::BlockScopeBlockMode;
use toplevel::TopLevelBlockMode;
// Only expose specific implementations of BlockLevelProcessor

/// Block-scope processing with [TopLevelBlockMode].
pub type TopLevelProcessor = BlockLevelProcessor<TopLevelBlockMode>;
/// Block-scope processing with [BlockScopeBlockMode].
pub type BlockScopeProcessor = BlockLevelProcessor<BlockScopeBlockMode>;

/// This struct handles block-mode processing.
///
/// Fake subclassing: the inner type T overrides block-mode processing behaviour in specific cases by implementing the BlockMode trait.
/// We can't require T to implement BlockMode directly here, because that would require making BlockMode "pub" (which for some stupid reason then makes us have to make all of the processor machinery like ProcStatus "pub" too, even if we never re-export BlockMode and make it accessible)
/// Instead we only implement [TokenProcessor] for `BlockLevelProcessor<T>` if `T` implements BlockMode.
pub struct BlockLevelProcessor<T> {
    inner: T,
    expects_n_blank_lines_after: Option<BlockModeElem>,
}

/// This trait overrides behaviour of the BlockLevelProcessor in specific cases.
trait BlockMode {
    fn on_close_scope(&mut self, py: Python, tok: TTToken, data: &str) -> TTResult<ProcStatus>;
    fn on_eof(&mut self, py: Python, tok: TTToken) -> TTResult<ProcStatus>;

    /// On receiving a block that is a header
    ///
    /// Must
    fn on_header(&mut self, py: Python, header: PyTcRef<Header>) -> TTResult<()>;
    /// On receiving a block that isn't a header
    fn on_block(&mut self, py: Python, block: &Bound<'_, PyAny>) -> TTResult<()>;
}
#[allow(private_bounds)]
impl<T: BlockMode> BlockLevelProcessor<T> {
    /// Disambiguate between a Header block or a plain old regular block, and pass it to the inner.
    fn on_block_or_header_or_blocks(
        &mut self,
        py: Python,
        block: &Bound<'_, PyAny>,
    ) -> TTResult<()> {
        if let Ok(blocks) = block.downcast::<Blocks>() {
            for block in blocks.borrow().0.list(py) {
                self.on_block_or_header_or_blocks(py, &block)?
            }
            Ok(())
        } else if let Ok(header) = PyTcRef::of(block) {
            self.inner.on_header(py, header)
        } else {
            self.inner.on_block(py, block)
        }
    }
}
/// The implementation of block-level token processing for all BlockLevelProcessors.
impl<T: BlockMode> TokenProcessor for BlockLevelProcessor<T> {
    fn process_token(
        &mut self,
        py: Python,
        _py_env: UserPythonEnv,
        tok: TTToken,
        data: &str,
    ) -> TTResult<ProcStatus> {
        // This builder may receive tokens from inner files.
        // It always returns an error, [ProcStatus::Continue], or [ProcStatus::PushProcessor] on non-EOF tokens
        // as long as [BlockTokenProcessor::on_close_scope] always does the same.
        // When receiving EOF it returns [ProcStatus::PopAndReprocessToken].
        // This fulfils the contract for [TokenProcessor::process_token].
        if self.expects_n_blank_lines_after.is_some() {
            match tok {
                TTToken::Escaped(span, Escapable::Newline) => {
                    Err(TTSyntaxError::EscapedNewlineInBlockMode { newline: span }.into())
                }
                TTToken::Whitespace(_) => Ok(ProcStatus::Continue),
                TTToken::Newline(_) => {
                    self.expects_n_blank_lines_after = None;
                    Ok(ProcStatus::Continue)
                }

                TTToken::Hashes(_, _) => Ok(ProcStatus::PushProcessor(rc_refcell(
                    CommentProcessor::new(),
                ))),

                // A scope close is not counted as "content" for our sake.
                TTToken::ScopeClose(_) => self.inner.on_close_scope(py, tok, data),

                TTToken::EOF(_) => self.inner.on_eof(py, tok),

                _ => Err(TTSyntaxError::InsufficientBlockSeparation {
                    last_block: std::mem::take(&mut self.expects_n_blank_lines_after).expect(
                        "This function is only called when \
                             self.expects_n_blank_lines_after.is_some()",
                    ),
                    next_block_start: BlockModeElem::AnyToken(tok.token_span()),
                })?,
            }
        } else {
            match tok {
                TTToken::Escaped(span, Escapable::Newline) => {
                    Err(TTSyntaxError::EscapedNewlineInBlockMode { newline: span }.into())
                }
                TTToken::Whitespace(_) | TTToken::Newline(_) => Ok(ProcStatus::Continue),

                TTToken::Hashes(_, _) => Ok(ProcStatus::PushProcessor(rc_refcell(
                    CommentProcessor::new(),
                ))),

                // Because this may return Inline we *always* have to be able to handle inlines at top scope.
                TTToken::CodeOpen(start_span, n_brackets) => Ok(ProcStatus::PushProcessor(
                    rc_refcell(CodeProcessor::new(start_span, n_brackets)),
                )),

                TTToken::ScopeOpen(start_span) => Ok(ProcStatus::PushProcessor(rc_refcell(
                    AmbiguousScopeProcessor::new(start_span, NoResolveCallbacks()),
                ))),

                TTToken::RawScopeOpen(start_span, n_opening) => Ok(ProcStatus::PushProcessor(
                    rc_refcell(RawStringProcessor::new(start_span, n_opening)),
                )),

                // Other escaped content, lone backslash, hyphens and dashes, and any other text are all treated as content
                TTToken::Escaped(text_span, _)
                | TTToken::Backslash(text_span)
                | TTToken::HyphenMinuses(text_span, _)
                | TTToken::EnDash(text_span)
                | TTToken::EmDash(text_span)
                | TTToken::OtherText(text_span) => Ok(ProcStatus::PushProcessor(rc_refcell(
                    ParagraphProcessor::new_with_starting_text(
                        py,
                        tok.stringify_escaped(data),
                        text_span,
                    )?,
                ))),

                TTToken::CodeClose(span, _) => {
                    Err(TTSyntaxError::CodeCloseOutsideCode(span).into())
                }

                TTToken::RawScopeClose(span, _) => {
                    Err(TTSyntaxError::RawScopeCloseOutsideRawScope(span).into())
                }

                TTToken::ScopeClose(_) => self.inner.on_close_scope(py, tok, data),

                TTToken::EOF(_) => self.inner.on_eof(py, tok),
            }
        }
    }

    fn process_emitted_element(
        &mut self,
        py: Python,
        _py_env: UserPythonEnv,
        pushed: Option<EmittedElement>,
    ) -> TTResult<ProcStatus> {
        match pushed {
            Some((elem_ctx, elem)) => match elem {
                // Blocks must have been received from either
                // 1. a paragraph, which has seen a blank line and ended itself
                // 2. eval-brackets that directly emitted a block
                // 3. eval-brackets that took some argument (a block scope, an inline scope, or a raw scope) and emitted a block
                // 4. a manually opened block scope that was just closed
                // We always want a single clear line between block elements.
                // In the case of 1, the paragraph has processed a blank line already because that's what ends the paragraph.
                // In the cases of 2, 3, and 4 we need to clear a. the current line and b. the next line, so set expects_blank_line=2
                DocElement::Block(BlockElem::Para(p)) => {
                    // The paragraph has already received a fully blank line.
                    // It's ok to set this flag high based on pushes from inner subfiles - it goes high when the subfile finishes anyway.
                    self.expects_n_blank_lines_after = None;
                    self.inner.on_block(py, p.into_any().bind(py))?;
                    Ok(ProcStatus::Continue)
                }
                DocElement::Block(BlockElem::Blocks(blocks)) => {
                    self.expects_n_blank_lines_after = Some(BlockModeElem::Blocks(elem_ctx));
                    self.on_block_or_header_or_blocks(py, blocks.bind(py))?;
                    Ok(ProcStatus::Continue)
                }
                DocElement::Block(BlockElem::FromCode(block)) => {
                    // Set expects_blank_line to one i.e. no content on the current line.
                    // It's ok to set this flag high based on pushes from inner subfiles - it goes high when the subfile finishes anyway.
                    self.expects_n_blank_lines_after =
                        Some(BlockModeElem::BlockFromCode(elem_ctx.full_span()));
                    self.on_block_or_header_or_blocks(py, block.bind(py))?;
                    Ok(ProcStatus::Continue)
                }
                // If we get an inline, start building a paragraph with it
                DocElement::Inline(inline) => Ok(ProcStatus::PushProcessor(rc_refcell(
                    ParagraphProcessor::new_with_inline(py, inline.bind(py), elem_ctx)?,
                ))),
            },
            None => Ok(ProcStatus::Continue),
        }
    }

    fn on_emitted_source_inside(&mut self, _code_emitting_source: ParseContext) -> TTResult<()> {
        // The tokens from this file will be passed through directly to us until we open new builders in its stack.
        // Allow the new file to start directly with content if it chooses.
        self.expects_n_blank_lines_after = None;
        Ok(())
    }

    fn on_emitted_source_closed(&mut self, inner_source_emitted_by: ParseSpan) {
        // An inner file must have come from emitted code - a blank line must be seen before any new content after code emitting a file
        self.expects_n_blank_lines_after =
            Some(BlockModeElem::SourceFromCode(inner_source_emitted_by));
    }
}
