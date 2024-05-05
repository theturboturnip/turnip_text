use pyo3::prelude::*;

use crate::{
    interpreter::{
        error::{
            syntax::{BlockModeElem, InlineModeContext, TTSyntaxError},
            TTResult,
        },
        lexer::TTToken,
        UserPythonEnv,
    },
    util::{ParseContext, ParseSpan},
};

use super::{
    block::BlockScopeProcessor, comment::CommentProcessor, inline::KnownInlineScopeProcessor,
    rc_refcell, EmittedElement, ProcStatus, TokenProcessor,
};

pub trait OnResolveAmbiguousScope {
    fn got_block_scope(self, py: Python, scope_open: ParseSpan) -> TTResult<()>;
    fn got_inline_scope(self, py: Python, scope_open: ParseSpan) -> TTResult<()>;
}

pub struct NoResolveCallbacks();
impl OnResolveAmbiguousScope for NoResolveCallbacks {
    fn got_block_scope(self, _py: Python, _scope_open: ParseSpan) -> TTResult<()> {
        Ok(())
    }

    fn got_inline_scope(self, _py: Python, _scope_open: ParseSpan) -> TTResult<()> {
        Ok(())
    }
}

/// This builder is initially started with a ScopeOpen token that may be a block scope open (followed by "\s*\n") or an inline scope open (followed by \s*[^\n]).
/// It starts out [AmbiguousScopeProcessor::Undecided], then based on the following tokens either decides on [AmbiguousScopeProcessor::Block] or [AmbiguousScopeProcessor::Inline] and from then on acts as exactly [BlockScopeProcessor] or [KnownInlineScopeProcessor] respectfully.
pub enum AmbiguousScopeProcessor<T: OnResolveAmbiguousScope> {
    Undecided { first_tok: ParseSpan, callbacks: T },
    Block(BlockScopeProcessor),
    Inline(KnownInlineScopeProcessor),
}
impl<T: OnResolveAmbiguousScope> AmbiguousScopeProcessor<T> {
    pub fn new(first_tok: ParseSpan, callbacks: T) -> Self {
        Self::Undecided {
            first_tok,
            callbacks,
        }
    }
}
impl<T: OnResolveAmbiguousScope> TokenProcessor for AmbiguousScopeProcessor<T> {
    fn process_token(
        &mut self,
        py: Python,
        py_env: UserPythonEnv,
        tok: TTToken,
        data: &str,
    ) -> TTResult<ProcStatus> {
        match self {
            AmbiguousScopeProcessor::Undecided {
                first_tok,
                callbacks: _,
            } => match tok {
                // This builder does not directly emit new source files, so it cannot receive tokens from inner files
                // while in the Undecided state.
                // When receiving EOF it returns an error.
                // This fulfils the contract for [TokenProcessor::process_token].
                TTToken::Whitespace(_) => Ok(ProcStatus::Continue),
                TTToken::EOF(eof_span) => Err(TTSyntaxError::EndedInsideScope {
                    scope_start: *first_tok,
                    eof_span,
                }
                .into()),
                TTToken::Hashes(_, _) => Ok(ProcStatus::PushProcessor(rc_refcell(
                    CommentProcessor::new(),
                ))),
                // Got a newline, so it's a block scope
                TTToken::Newline(last_tok) => {
                    // Transition to a block builder
                    let block_builder = BlockScopeProcessor::new(py, *first_tok, last_tok)?;
                    // Block builder doesn't need to process the newline token specifically
                    // Swap ourselves out with the new state "i am a block builder"
                    let scope_start_tok = *first_tok;
                    let undecided =
                        std::mem::replace(self, AmbiguousScopeProcessor::Block(block_builder));
                    // call the callback to check if that's OK
                    // the old version of us must be Undecided
                    match undecided {
                        AmbiguousScopeProcessor::Undecided { callbacks, .. } => {
                            callbacks.got_block_scope(py, scope_start_tok)?
                        }
                        _ => unreachable!(),
                    }
                    Ok(ProcStatus::Continue)
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
                    let scope_start_tok = *first_tok;
                    let undecided =
                        std::mem::replace(self, AmbiguousScopeProcessor::Inline(inline_builder));
                    // call the callback to check if that's OK
                    // the old version of us must be Undecided
                    match undecided {
                        AmbiguousScopeProcessor::Undecided { callbacks, .. } => {
                            callbacks.got_inline_scope(py, scope_start_tok)?
                        }
                        _ => unreachable!(),
                    }
                    Ok(res)
                }
            },
            AmbiguousScopeProcessor::Block(block) => block.process_token(py, py_env, tok, data),
            AmbiguousScopeProcessor::Inline(inline) => inline.process_token(py, py_env, tok, data),
        }
    }

    fn process_emitted_element(
        &mut self,
        py: Python,
        py_env: UserPythonEnv,
        pushed: Option<EmittedElement>,
    ) -> TTResult<ProcStatus> {
        match self {
            AmbiguousScopeProcessor::Undecided { .. } => {
                assert!(
                    pushed.is_none(),
                    "AmbiguousScopeProcessor::Undecided does not push any builders except \
                     comments thus cannot receive non-None pushed items"
                );
                Ok(ProcStatus::Continue)
            }
            AmbiguousScopeProcessor::Block(block) => {
                block.process_emitted_element(py, py_env, pushed)
            }
            AmbiguousScopeProcessor::Inline(inline) => {
                inline.process_emitted_element(py, py_env, pushed)
            }
        }
    }

    fn on_emitted_source_inside(&mut self, code_emitting_source: ParseContext) -> TTResult<()> {
        match self {
            AmbiguousScopeProcessor::Undecided { .. } => {
                unreachable!(
                    "AmbiguousScopeProcessor::Undecided does not push any builders except \
                     comments and thus cannot have source code emitted inside it"
                )
            }
            AmbiguousScopeProcessor::Block(block) => {
                block.on_emitted_source_inside(code_emitting_source)
            }
            AmbiguousScopeProcessor::Inline(inline) => {
                inline.on_emitted_source_inside(code_emitting_source)
            }
        }
    }

    fn on_emitted_source_closed(&mut self, inner_source_emitted_by: ParseSpan) {
        match self {
            AmbiguousScopeProcessor::Undecided { .. } => {
                unreachable!(
                    "AmbiguousScopeProcessor::Undecided does not push any builders except \
                     comments and thus cannot have source code emitted inside it"
                )
            }
            AmbiguousScopeProcessor::Block(block) => {
                block.on_emitted_source_closed(inner_source_emitted_by)
            }
            AmbiguousScopeProcessor::Inline(inline) => {
                inline.on_emitted_source_closed(inner_source_emitted_by)
            }
        }
    }
}

/// Parser for a scope which based on context *should* be inline, i.e. if you encounter no content before a newline then you must throw an error.
pub enum InlineLevelAmbiguousScopeProcessor {
    Undecided {
        preceding_inline: InlineModeContext,
        start_of_line: bool,
        scope_ctx: ParseContext,
    },
    Known(KnownInlineScopeProcessor),
}
impl InlineLevelAmbiguousScopeProcessor {
    pub fn new(
        preceding_inline: InlineModeContext,
        start_of_line: bool,
        start_span: ParseSpan,
    ) -> Self {
        Self::Undecided {
            preceding_inline,
            start_of_line,
            scope_ctx: ParseContext::new(start_span, start_span),
        }
    }
}
impl TokenProcessor for InlineLevelAmbiguousScopeProcessor {
    fn process_token(
        &mut self,
        py: Python,
        py_env: UserPythonEnv,
        tok: TTToken,
        data: &str,
    ) -> TTResult<ProcStatus> {
        match self {
            InlineLevelAmbiguousScopeProcessor::Undecided {
                preceding_inline,
                start_of_line,
                scope_ctx,
            } => match tok {
                TTToken::Newline(_) => match preceding_inline {
                    InlineModeContext::Paragraph(preceding_para) => {
                        if *start_of_line {
                            Err(TTSyntaxError::InsufficientBlockSeparation {
                                last_block: BlockModeElem::Para(*preceding_para),
                                // The start of the next block is our *first* token, not the current token - that's just a newline
                                next_block_start: BlockModeElem::AnyToken(scope_ctx.first_tok()),
                            })?
                        } else {
                            // TODO test the case where you open a paragraph, then in the middle of a line you insert a block-scope-open - the preceding_para context should be the whole para up to the block-scope-open
                            Err(TTSyntaxError::BlockScopeOpenedInInlineMode {
                                inl_mode: preceding_inline.clone(),
                                block_scope_open: scope_ctx.first_tok(),
                            })?
                        }
                    }
                    InlineModeContext::InlineScope { .. } => {
                        // TODO test the case where you open a paragraph, then in the middle of a line you insert a block-scope-open *inside an inline scope* - the preceding_para context should be the whole para including that enclosing inline scope
                        Err(TTSyntaxError::BlockScopeOpenedInInlineMode {
                            inl_mode: preceding_inline.clone(),
                            block_scope_open: scope_ctx.first_tok(),
                        })?
                    }
                },
                // Ignore whitespace on the first line
                TTToken::Whitespace(_) => Ok(ProcStatus::Continue),
                // This inevitably will fail, because we won't receive any content other than the newline
                TTToken::Hashes(_, _) => Ok(ProcStatus::PushProcessor(rc_refcell(
                    CommentProcessor::new(),
                ))),
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
            InlineLevelAmbiguousScopeProcessor::Known(k) => k.process_token(py, py_env, tok, data),
        }
    }

    fn process_emitted_element(
        &mut self,
        py: Python,
        py_env: UserPythonEnv,
        pushed: Option<EmittedElement>,
    ) -> TTResult<ProcStatus> {
        match self {
            InlineLevelAmbiguousScopeProcessor::Undecided { .. } => {
                assert!(
                    pushed.is_none(),
                    "InlineLevelAmbiguousScopeProcessor::Undecided does not push any builders \
                     except comments thus cannot receive non-None pushed items"
                );
                Ok(ProcStatus::Continue)
            }
            InlineLevelAmbiguousScopeProcessor::Known(k) => {
                k.process_emitted_element(py, py_env, pushed)
            }
        }
    }

    fn on_emitted_source_inside(&mut self, code_emitting_source: ParseContext) -> TTResult<()> {
        match self {
            InlineLevelAmbiguousScopeProcessor::Undecided { .. } => unreachable!(
                "InlineLevelAmbiguousScopeProcessor doesn't spawn non-comment builders in \
                 Undecided mode, so can't get a source emitted from them"
            ),
            InlineLevelAmbiguousScopeProcessor::Known(k) => {
                k.on_emitted_source_inside(code_emitting_source)
            }
        }
    }

    fn on_emitted_source_closed(&mut self, inner_source_emitted_by: ParseSpan) {
        match self {
            InlineLevelAmbiguousScopeProcessor::Undecided { .. } => unreachable!(
                "InlineLevelAmbiguousScopeProcessor doesn't spawn non-comment builders in \
                 Undecided mode, so can't get a source emitted from them"
            ),
            InlineLevelAmbiguousScopeProcessor::Known(k) => {
                k.on_emitted_source_closed(inner_source_emitted_by)
            }
        }
    }
}
