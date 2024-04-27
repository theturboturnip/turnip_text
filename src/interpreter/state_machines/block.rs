use pyo3::{prelude::*, types::PyDict};

use crate::{
    error::{
        interp::{BlockModeElem, InterpError, MapContextlessResult},
        TurnipTextContextlessResult,
    },
    interpreter::InterimDocumentStructure,
    lexer::{Escapable, TTToken},
    python::{
        interop::{BlockScope, DocSegment, DocSegmentHeader},
        typeclass::PyTcRef,
    },
    util::{ParseContext, ParseSpan},
};

use super::{
    ambiguous_scope::AmbiguousScopeProcessor,
    code::CodeProcessor,
    comment::CommentProcessor,
    inline::{ParagraphProcessor, RawStringProcessor},
    py_internal_alloc, rc_refcell, BlockElem, DocElement, EmittedElement, ProcStatus,
    TokenProcessor,
};

// Only expose specific implementations of BlockLevelProcessor
pub type TopLevelProcessor = BlockLevelProcessor<TopLevelBlockMode>;
pub type BlockScopeProcessor = BlockLevelProcessor<BlockScopeBlockMode>;

/// This struct handles block-mode processing.
///
/// Fake subclassing: the inner type T overrides block-mode processing behaviour in specific cases by implementing the BlockMode trait.
/// We can't require T to implement BlockMode directly here, because that would require making BlockMode "pub" (which for some stupid reason then makes us have to make all of the processor machinery like ProcStatus "pub" too, even if we never re-export BlockMode and make it accessible)
/// Instead we only implement [TokenProcessor] for `BlockLevelProcessor<T>` if `T` implements BlockMode.
pub struct BlockLevelProcessor<T> {
    inner: T,
    expects_n_blank_lines_after: Option<(u8, BlockModeElem)>,
}

/// This trait overrides behaviour of the BlockLevelProcessor in specific cases.
trait BlockMode {
    fn on_close_scope(
        &mut self,
        py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<ProcStatus>;
    fn on_eof(&mut self, py: Python, tok: TTToken) -> TurnipTextContextlessResult<ProcStatus>;

    fn on_header(
        &mut self,
        py: Python,
        header: PyTcRef<DocSegmentHeader>,
        header_ctx: ParseContext,
    ) -> TurnipTextContextlessResult<ProcStatus>;
    fn on_block(
        &mut self,
        py: Python,
        block: BlockElem,
        block_ctx: ParseContext,
    ) -> TurnipTextContextlessResult<ProcStatus>;
}

/// At the top level of the document, headers are allowed and manipulate the InterimDocumentStructure.
pub struct TopLevelBlockMode {
    structure: InterimDocumentStructure,
}
impl BlockLevelProcessor<TopLevelBlockMode> {
    pub fn new(py: Python) -> PyResult<Self> {
        Ok(Self {
            inner: TopLevelBlockMode {
                structure: InterimDocumentStructure::new(py)?,
            },
            expects_n_blank_lines_after: None,
        })
    }
    pub fn finalize(mut self, py: Python<'_>) -> TurnipTextContextlessResult<Py<DocSegment>> {
        self.inner
            .structure
            .pop_segments_until_less_than(py, i64::MIN)?;
        self.inner.structure.finalize(py).err_as_internal(py)
    }
}
impl BlockMode for TopLevelBlockMode {
    fn on_close_scope(
        &mut self,
        _py: Python,
        tok: TTToken,
        _data: &str,
    ) -> TurnipTextContextlessResult<ProcStatus> {
        // This builder may receive tokens from inner files.
        // It always returns an error.
        // This fulfils the contract for [TokenProcessor::process_token].
        Err(InterpError::BlockScopeCloseOutsideScope(tok.token_span()).into())
    }

    // When EOF comes, we don't produce anything to bubble up - there's nothing above us!
    fn on_eof(&mut self, _py: Python, _tok: TTToken) -> TurnipTextContextlessResult<ProcStatus> {
        // This is the only exception to the contract for [TokenProcessor::process_token].
        // There is never a builder above this one, so there is nothing that can reprocess the token.
        Ok(ProcStatus::Continue)
    }

    fn on_header(
        &mut self,
        py: Python,
        header: PyTcRef<DocSegmentHeader>,
        _header_ctx: ParseContext,
    ) -> TurnipTextContextlessResult<ProcStatus> {
        self.structure.push_segment_header(py, header)?;
        Ok(ProcStatus::Continue)
    }

    fn on_block(
        &mut self,
        py: Python,
        block: BlockElem,
        _block_ctx: ParseContext,
    ) -> TurnipTextContextlessResult<ProcStatus> {
        self.structure.push_to_topmost_block(py, block.as_ref(py))?;
        Ok(ProcStatus::Continue)
    }
}

pub struct BlockScopeBlockMode {
    ctx: ParseContext,
    block_scope: Py<BlockScope>,
}
impl BlockMode for BlockScopeBlockMode {
    fn on_close_scope(
        &mut self,
        py: Python,
        tok: TTToken,
        _data: &str,
    ) -> TurnipTextContextlessResult<ProcStatus> {
        // This builder may receive tokens from inner files.
        // If it receives a token from an inner file, it returns an error.
        // This fulfils the contract for [TokenProcessor::process_token].
        if !self.ctx.try_extend(&tok.token_span()) {
            // Closing block scope from different file
            // This must be a block-level scope close, because if an unbalanced scope close appeared in inline mode it would already have errored and not bubbled out.
            Err(InterpError::BlockScopeCloseOutsideScope(tok.token_span()).into())
        } else {
            Ok(ProcStatus::Pop(Some((
                self.ctx,
                BlockElem::BlockScope(self.block_scope.clone_ref(py)).into(),
            ))))
        }
    }

    fn on_eof(&mut self, _py: Python, tok: TTToken) -> TurnipTextContextlessResult<ProcStatus> {
        Err(InterpError::EndedInsideScope {
            scope_start: self.ctx.first_tok(),
            eof_span: tok.token_span(),
        }
        .into())
    }

    fn on_header(
        &mut self,
        _py: Python,
        header: PyTcRef<DocSegmentHeader>,
        header_ctx: ParseContext,
    ) -> TurnipTextContextlessResult<ProcStatus> {
        Err(InterpError::CodeEmittedHeaderInBlockScope {
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
    ) -> TurnipTextContextlessResult<ProcStatus> {
        self.block_scope
            .borrow_mut(py)
            .push_block(block.as_ref(py))
            .err_as_internal(py)?;
        Ok(ProcStatus::Continue)
    }
}
impl BlockLevelProcessor<BlockScopeBlockMode> {
    pub fn new(
        py: Python,
        first_tok: ParseSpan,
        last_tok: ParseSpan,
    ) -> TurnipTextContextlessResult<Self> {
        Ok(Self {
            inner: BlockScopeBlockMode {
                ctx: ParseContext::new(first_tok, last_tok),
                block_scope: py_internal_alloc(py, BlockScope::new_empty(py))?,
            },
            expects_n_blank_lines_after: None,
        })
    }
}

/// The implementation of block-level token processing for all BlockLevelProcessors.
impl<T: BlockMode> TokenProcessor for BlockLevelProcessor<T> {
    fn process_token(
        &mut self,
        py: Python,
        _py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<ProcStatus> {
        // This builder may receive tokens from inner files.
        // It always returns an error, [ProcStatus::Continue], or [ProcStatus::PushProcessor] on non-EOF tokens
        // as long as [BlockTokenProcessor::on_close_scope] always does the same.
        // When receiving EOF it returns [ProcStatus::PopAndReprocessToken].
        // This fulfils the contract for [TokenProcessor::process_token].
        if self.expects_n_blank_lines_after.is_some() {
            match tok {
                    TTToken::Escaped(span, Escapable::Newline) => {
                        Err(InterpError::EscapedNewlineOutsideParagraph { newline: span }.into())
                    }
                    TTToken::Whitespace(_) => Ok(ProcStatus::Continue),
                    TTToken::Newline(_) => {
                        self.expects_n_blank_lines_after =
                            match std::mem::take(&mut self.expects_n_blank_lines_after) {
                                Some((0, _)) => {
                                    unreachable!(
                                        "should never set expects_n_blank_lines_after = (0, _)"
                                    )
                                }
                                Some((1, _)) => None,
                                Some((n_lines, ctx)) => Some((n_lines - 1, ctx)),
                                None => None,
                            };
                        Ok(ProcStatus::Continue)
                    }

                    TTToken::Hashes(_, _) => {
                        Ok(ProcStatus::PushProcessor(rc_refcell(CommentProcessor::new())))
                    }

                    // A scope close is not counted as "content" for our sake.
                    TTToken::ScopeClose(_) => self.inner.on_close_scope(py, tok, data),

                    TTToken::EOF(_) => self.inner.on_eof(py, tok),

                    _ => Err(InterpError::InsufficientBlockSeparation {
                        last_block: std::mem::take(&mut self.expects_n_blank_lines_after)
                            .expect(
                                "This function is only called when self.expects_n_blank_lines_after.is_some()",
                            )
                            .1,
                        next_block_start: BlockModeElem::AnyToken(tok.token_span()),
                    })?,
                }
        } else {
            match tok {
                TTToken::Escaped(span, Escapable::Newline) => {
                    Err(InterpError::EscapedNewlineOutsideParagraph { newline: span }.into())
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
                    AmbiguousScopeProcessor::new(start_span),
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

                TTToken::CodeClose(span, _) => Err(InterpError::CodeCloseOutsideCode(span).into()),

                TTToken::RawScopeClose(span, _) => {
                    Err(InterpError::RawScopeCloseOutsideRawScope(span).into())
                }

                TTToken::ScopeClose(_) => self.inner.on_close_scope(py, tok, data),

                TTToken::EOF(_) => self.inner.on_eof(py, tok),
            }
        }
    }

    fn process_emitted_element(
        &mut self,
        py: Python,
        _py_env: &PyDict,
        pushed: Option<EmittedElement>,
    ) -> TurnipTextContextlessResult<ProcStatus> {
        match pushed {
            Some((elem_ctx, elem)) => match elem {
                DocElement::HeaderFromCode(header) => {
                    // This must have been received from either
                    // 1. eval-brackets that directly emitted a header
                    // 2. eval-brackets that took some argument (a block scope, an inline scope, or a raw scope) and emitted a header
                    // We don't want any content between now and the end of the line, because that would be emitted into a new block and it would look confusing.
                    // Thus, set expects_blank_line to *two* blank lines i.e. no content on the current line, and then a full line without content
                    // It's ok to set this flag high based on pushes from inner subfiles - it goes high when the subfile finishes anyway.
                    self.expects_n_blank_lines_after =
                        Some((2, BlockModeElem::HeaderFromCode(elem_ctx.full_span())));
                    self.inner.on_header(py, header, elem_ctx)
                }
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
                    self.inner
                        .on_block(py, BlockElem::Para(p.clone_ref(py)), elem_ctx)
                }
                DocElement::Block(block) => {
                    // Set expects_blank_line to *two* blank lines i.e. no content on the current line, and then a full line without content
                    // It's ok to set this flag high based on pushes from inner subfiles - it goes high when the subfile finishes anyway.
                    self.expects_n_blank_lines_after = Some((2, (elem_ctx, &block).into()));
                    self.inner.on_block(py, block, elem_ctx)
                }
                // If we get an inline, start building a paragraph with it
                DocElement::Inline(inline) => Ok(ProcStatus::PushProcessor(rc_refcell(
                    ParagraphProcessor::new_with_inline(py, inline.as_ref(py), elem_ctx)?,
                ))),
            },
            None => Ok(ProcStatus::Continue),
        }
    }

    fn on_emitted_source_inside(
        &mut self,
        _code_emitting_source: ParseContext,
    ) -> TurnipTextContextlessResult<()> {
        // The tokens from this file will be passed through directly to us until we open new builders in its stack.
        // Allow the new file to start directly with content if it chooses.
        self.expects_n_blank_lines_after = None;
        Ok(())
    }

    fn on_emitted_source_closed(&mut self, inner_source_emitted_by: ParseSpan) {
        // An inner file must have come from emitted code - a blank line must be seen before any new content after code emitting a file
        self.expects_n_blank_lines_after =
            Some((2, BlockModeElem::SourceFromCode(inner_source_emitted_by)));
    }
}
