use std::{cell::RefCell, rc::Rc};

use pyo3::{prelude::*, types::PyDict};

use crate::{
    error::{
        interp::{BlockModeElem, InlineModeContext, InterpError},
        TurnipTextContextlessResult,
    },
    lexer::TTToken,
    util::{ParseContext, ParseSpan},
};

use super::{
    block::BlockScopeProcessor, comment::CommentFromTokens, inline::KnownInlineScopeProcessor,
    rc_refcell, BuildFromTokens, BuildStatus, PushToNextLevel,
};

/// This builder is initially started with a ScopeOpen token that may be a block scope open (followed by "\s*\n") or an inline scope open (followed by \s*[^\n]).
/// It starts out [BlockOrInlineScopeFromTokens::Undecided], then based on the following tokens either decides on [BlockOrInlineScopeFromTokens::Block] or [BlockOrInlineScopeFromTokens::Inline] and from then on acts as exactly [BlockScopeFromTokens] or [InlineScopeFromTokens] respectfully.
pub enum BlockLevelAmbiguousScope {
    Undecided { first_tok: ParseSpan },
    Block(BlockScopeProcessor),
    Inline(KnownInlineScopeProcessor),
}
impl BlockLevelAmbiguousScope {
    pub fn new(first_tok: ParseSpan) -> Rc<RefCell<Self>> {
        rc_refcell(Self::Undecided { first_tok })
    }
}
impl BuildFromTokens for BlockLevelAmbiguousScope {
    fn process_token(
        &mut self,
        py: Python,
        py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        match self {
            BlockLevelAmbiguousScope::Undecided { first_tok } => match tok {
                // This builder does not directly emit new source files, so it cannot receive tokens from inner files
                // while in the Undecided state.
                // When receiving EOF it returns an error.
                // This fulfils the contract for [BuildFromTokens::process_token].
                TTToken::Whitespace(_) => Ok(BuildStatus::Continue),
                TTToken::EOF(eof_span) => Err(InterpError::EndedInsideScope {
                    scope_start: *first_tok,
                    eof_span,
                }
                .into()),
                TTToken::Newline(last_tok) => {
                    // Transition to a block builder
                    let block_builder = BlockScopeProcessor::new(py, *first_tok, last_tok)?;
                    // Block builder doesn't need to process the newline token specifically
                    // Swap ourselves out with the new state "i am a block builder"
                    let _ = std::mem::replace(self, BlockLevelAmbiguousScope::Block(block_builder));
                    Ok(BuildStatus::Continue)
                }
                TTToken::Hashes(_, _) => {
                    Ok(BuildStatus::StartInnerBuilder(CommentFromTokens::new()))
                }
                _ => {
                    // Transition to an inline builder
                    let mut inline_builder = KnownInlineScopeProcessor::new(
                        py,
                        // This has not been preceded by any inline content
                        None,
                        ParseContext::new(*first_tok, tok.token_span()),
                    )?;
                    // Make sure it knows about the new token
                    let res = inline_builder.process_token(py, py_env, tok, data)?;
                    // Swap ourselves out with the new state "i am an inline builder"
                    let _ =
                        std::mem::replace(self, BlockLevelAmbiguousScope::Inline(inline_builder));
                    Ok(res)
                }
            },
            BlockLevelAmbiguousScope::Block(block) => block.process_token(py, py_env, tok, data),
            BlockLevelAmbiguousScope::Inline(inline) => inline.process_token(py, py_env, tok, data),
        }
    }

    fn process_push_from_inner_builder(
        &mut self,
        py: Python,
        py_env: &PyDict,
        pushed: Option<PushToNextLevel>,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        match self {
            BlockLevelAmbiguousScope::Undecided { .. } => {
                assert!(pushed.is_none(), "BlockOrInlineScopeFromTokens::Undecided does not push any builders except comments thus cannot receive non-None pushed items");
                Ok(BuildStatus::Continue)
            }
            BlockLevelAmbiguousScope::Block(block) => {
                block.process_push_from_inner_builder(py, py_env, pushed)
            }
            BlockLevelAmbiguousScope::Inline(inline) => {
                inline.process_push_from_inner_builder(py, py_env, pushed)
            }
        }
    }

    fn on_emitted_source_inside(
        &mut self,
        code_emitting_source: ParseContext,
    ) -> TurnipTextContextlessResult<()> {
        match self {
            BlockLevelAmbiguousScope::Undecided { .. } => {
                unreachable!("BlockOrInlineScopeFromTokens::Undecided does not push any builders except comments and thus cannot have source code emitted inside it")
            }
            BlockLevelAmbiguousScope::Block(block) => {
                block.on_emitted_source_inside(code_emitting_source)
            }
            BlockLevelAmbiguousScope::Inline(inline) => {
                inline.on_emitted_source_inside(code_emitting_source)
            }
        }
    }

    fn on_emitted_source_closed(&mut self, inner_source_emitted_by: ParseSpan) {
        match self {
            BlockLevelAmbiguousScope::Undecided { .. } => {
                unreachable!("BlockOrInlineScopeFromTokens::Undecided does not push any builders except comments and thus cannot have source code emitted inside it")
            }
            BlockLevelAmbiguousScope::Block(block) => {
                block.on_emitted_source_closed(inner_source_emitted_by)
            }
            BlockLevelAmbiguousScope::Inline(inline) => {
                inline.on_emitted_source_closed(inner_source_emitted_by)
            }
        }
    }
}

/// Parser for a scope which based on context *should* be inline, i.e. if you encounter no content before a newline then you must throw an error.
pub enum InlineLevelAmbiguousScope {
    Undecided {
        preceding_inline: InlineModeContext,
        start_of_line: bool,
        scope_ctx: ParseContext,
    },
    Known(KnownInlineScopeProcessor),
}
impl InlineLevelAmbiguousScope {
    pub fn new(
        preceding_inline: InlineModeContext,
        start_of_line: bool,
        start_span: ParseSpan,
    ) -> Rc<RefCell<Self>> {
        rc_refcell(Self::Undecided {
            preceding_inline,
            start_of_line,
            scope_ctx: ParseContext::new(start_span, start_span),
        })
    }
}
impl BuildFromTokens for InlineLevelAmbiguousScope {
    fn process_token(
        &mut self,
        py: Python,
        py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        match self {
            InlineLevelAmbiguousScope::Undecided {
                preceding_inline,
                start_of_line,
                scope_ctx,
            } => match tok {
                TTToken::Newline(_) => match preceding_inline {
                    InlineModeContext::Paragraph(preceding_para) => {
                        if *start_of_line {
                            Err(InterpError::InsufficientBlockSeparation {
                                last_block: BlockModeElem::Para(*preceding_para),
                                // The start of the next block is our *first* token, not the current token - that's just a newline
                                next_block_start: BlockModeElem::AnyToken(scope_ctx.first_tok()),
                            })?
                        } else {
                            // TODO test the case where you open a paragraph, then in the middle of a line you insert a block-scope-open - the preceding_para context should be the whole para up to the block-scope-open
                            Err(InterpError::BlockScopeOpenedInInlineMode {
                                inl_mode: preceding_inline.clone(),
                                block_scope_open: scope_ctx.first_tok(),
                            })?
                        }
                    }
                    InlineModeContext::InlineScope { .. } => {
                        // TODO test the case where you open a paragraph, then in the middle of a line you insert a block-scope-open *inside an inline scope* - the preceding_para context should be the whole para including that enclosing inline scope
                        Err(InterpError::BlockScopeOpenedInInlineMode {
                            inl_mode: preceding_inline.clone(),
                            block_scope_open: scope_ctx.first_tok(),
                        })?
                    }
                },
                // Ignore whitespace on the first line
                TTToken::Whitespace(_) => Ok(BuildStatus::Continue),
                // This inevitably will fail, because we won't receive any content other than the newline
                TTToken::Hashes(_, _) => {
                    Ok(BuildStatus::StartInnerBuilder(CommentFromTokens::new()))
                }
                // In any other case we're creating *some* content - we must be in an inline scope
                _ => {
                    // Transition to an inline builder
                    let mut inline_builder = KnownInlineScopeProcessor::new(
                        py,
                        Some(preceding_inline.clone()),
                        *scope_ctx,
                    )?;
                    // Make sure it knows about the new token
                    let res = inline_builder.process_token(py, py_env, tok, data)?;
                    // Swap ourselves out with the new state "i am an inline builder"
                    let _ = std::mem::replace(self, Self::Known(inline_builder));
                    Ok(res)
                }
            },
            InlineLevelAmbiguousScope::Known(k) => k.process_token(py, py_env, tok, data),
        }
    }

    fn process_push_from_inner_builder(
        &mut self,
        py: Python,
        py_env: &PyDict,
        pushed: Option<PushToNextLevel>,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        match self {
            InlineLevelAmbiguousScope::Undecided { .. } => {
                assert!(pushed.is_none(), "ScopeWhichShouldBeInline::Undecided does not push any builders except comments thus cannot receive non-None pushed items");
                Ok(BuildStatus::Continue)
            }
            InlineLevelAmbiguousScope::Known(k) => {
                k.process_push_from_inner_builder(py, py_env, pushed)
            }
        }
    }

    fn on_emitted_source_inside(
        &mut self,
        code_emitting_source: ParseContext,
    ) -> TurnipTextContextlessResult<()> {
        match self {
            InlineLevelAmbiguousScope::Undecided { .. } => unreachable!("ScopeWhichShouldBeInline doesn't spawn non-comment builders in Undecided mode, so can't get a source emitted from them"),
            InlineLevelAmbiguousScope::Known(k) => k.on_emitted_source_inside(code_emitting_source),
        }
    }

    fn on_emitted_source_closed(&mut self, inner_source_emitted_by: ParseSpan) {
        match self {
            InlineLevelAmbiguousScope::Undecided { .. } => unreachable!("ScopeWhichShouldBeInline doesn't spawn non-comment builders in Undecided mode, so can't get a source emitted from them"),
            InlineLevelAmbiguousScope::Known(k) => k.on_emitted_source_closed(inner_source_emitted_by),
        }
    }
}
