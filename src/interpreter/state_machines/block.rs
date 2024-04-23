use std::{cell::RefCell, rc::Rc};

use pyo3::{prelude::*, types::PyDict};

use crate::{
    error::{
        interp::{BlockModeElem, InterpError, MapContextlessResult},
        TurnipTextContextlessError, TurnipTextContextlessResult,
    },
    interpreter::InterimDocumentStructure,
    lexer::{Escapable, TTToken},
    python::interop::{BlockScope, DocSegment},
    util::{ParseContext, ParseSpan},
};

use super::{
    code::CodeFromTokens,
    comment::CommentFromTokens,
    inline::{KnownInlineScopeFromTokens, ParagraphFromTokens, RawStringFromTokens},
    py_internal_alloc, rc_refcell, BlockElem, BuildFromTokens, BuildStatus, DocElement,
    PushToNextLevel,
};

trait BlockTokenProcessor {
    fn expects_new_line(&self) -> bool;
    fn on_new_line_finish(&mut self);
    fn on_unexpected_token_while_expecting_new_line(
        &mut self,
        tok: TTToken,
    ) -> TurnipTextContextlessError;

    fn on_close_scope(
        &mut self,
        py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus>;
    fn on_eof(&mut self, py: Python, tok: TTToken) -> TurnipTextContextlessResult<BuildStatus>;

    fn process_block_level_token(
        &mut self,
        py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        // This builder may receive tokens from inner files.
        // It always returns an error, [BuildStatus::Continue], or [BuildStatus::StartInnerBuilder] on non-EOF tokens
        // as long as [BlockTokenProcessor::on_close_scope] always does the same.
        // When receiving EOF it returns [BuildStatus::DoneAndReprocessToken].
        // This fulfils the contract for [BuildFromTokens::process_token].
        if self.expects_new_line() {
            match tok {
                TTToken::Escaped(span, Escapable::Newline) => {
                    Err(InterpError::EscapedNewlineOutsideParagraph { newline: span }.into())
                }
                TTToken::Whitespace(_) => Ok(BuildStatus::Continue),
                TTToken::Newline(_) => {
                    self.on_new_line_finish();
                    Ok(BuildStatus::Continue)
                }

                TTToken::Hashes(_, _) => {
                    Ok(BuildStatus::StartInnerBuilder(CommentFromTokens::new()))
                }

                // A scope close is not counted as "content" for our sake.
                TTToken::ScopeClose(_) => self.on_close_scope(py, tok, data),

                TTToken::EOF(_) => self.on_eof(py, tok),

                _ => Err(self.on_unexpected_token_while_expecting_new_line(tok)),
            }
        } else {
            match tok {
                TTToken::Escaped(span, Escapable::Newline) => {
                    Err(InterpError::EscapedNewlineOutsideParagraph { newline: span }.into())
                }
                TTToken::Whitespace(_) | TTToken::Newline(_) => Ok(BuildStatus::Continue),

                TTToken::Hashes(_, _) => {
                    Ok(BuildStatus::StartInnerBuilder(CommentFromTokens::new()))
                }

                // Because this may return Inline we *always* have to be able to handle inlines at top scope.
                TTToken::CodeOpen(start_span, n_brackets) => Ok(BuildStatus::StartInnerBuilder(
                    CodeFromTokens::new(start_span, n_brackets),
                )),

                TTToken::ScopeOpen(start_span) => Ok(BuildStatus::StartInnerBuilder(
                    BlockLevelAmbiguousScope::new(start_span),
                )),

                TTToken::RawScopeOpen(start_span, n_opening) => Ok(BuildStatus::StartInnerBuilder(
                    RawStringFromTokens::new(start_span, n_opening),
                )),

                TTToken::Escaped(text_span, _)
                | TTToken::Backslash(text_span)
                | TTToken::OtherText(text_span) => Ok(BuildStatus::StartInnerBuilder(
                    ParagraphFromTokens::new_with_starting_text(
                        py,
                        tok.stringify_escaped(data),
                        text_span,
                    )?,
                )),

                TTToken::CodeClose(span, _) => Err(InterpError::CodeCloseOutsideCode(span).into()),

                TTToken::RawScopeClose(span, _) => {
                    Err(InterpError::RawScopeCloseOutsideRawScope(span).into())
                }

                TTToken::ScopeClose(_) => self.on_close_scope(py, tok, data),

                TTToken::EOF(_) => self.on_eof(py, tok),
            }
        }
    }
}

pub struct TopLevelDocumentBuilder {
    /// The current structure of the document, including toplevel content, segments, and the current block stacks (one block stack per included subfile)
    structure: InterimDocumentStructure,
    expects_n_blank_lines_after: Option<(u8, BlockModeElem)>,
}
impl TopLevelDocumentBuilder {
    pub fn new(py: Python) -> PyResult<Rc<RefCell<Self>>> {
        Ok(rc_refcell(Self {
            structure: InterimDocumentStructure::new(py)?,
            expects_n_blank_lines_after: None,
        }))
    }

    pub fn finalize(mut self, py: Python) -> TurnipTextContextlessResult<Py<DocSegment>> {
        self.structure.pop_segments_until_less_than(py, i64::MIN)?;
        self.structure.finalize(py).err_as_internal(py)
    }
}
impl BlockTokenProcessor for TopLevelDocumentBuilder {
    fn expects_new_line(&self) -> bool {
        self.expects_n_blank_lines_after.is_some()
    }
    fn on_new_line_finish(&mut self) {
        self.expects_n_blank_lines_after =
            match std::mem::take(&mut self.expects_n_blank_lines_after) {
                Some((0, _)) => {
                    unreachable!("should never set expects_n_blank_lines_after = (0, _)")
                }
                Some((1, _)) => None,
                Some((n_lines, ctx)) => Some((n_lines - 1, ctx)),
                None => None,
            };
    }
    fn on_unexpected_token_while_expecting_new_line(
        &mut self,
        tok: TTToken,
    ) -> TurnipTextContextlessError {
        InterpError::InsufficientBlockSeparation {
            last_block: std::mem::take(&mut self.expects_n_blank_lines_after)
                .expect(
                    "This function is only called when self.expects_n_blank_lines_after.is_some()",
                )
                .1,
            next_block_start: BlockModeElem::AnyToken(tok.token_span()),
        }
        .into()
    }

    fn on_close_scope(
        &mut self,
        _py: Python,
        tok: TTToken,
        _data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        // This builder may receive tokens from inner files.
        // It always returns an error.
        // This fulfils the contract for [BuildFromTokens::process_token].
        Err(InterpError::BlockScopeCloseOutsideScope(tok.token_span()).into())
    }

    // When EOF comes, we don't produce anything to bubble up - there's nothing above us!
    fn on_eof(&mut self, _py: Python, _tok: TTToken) -> TurnipTextContextlessResult<BuildStatus> {
        // This is the only exception to the contract for [BuildFromTokens::process_token].
        // There is never a builder above this one, so there is nothing that can reprocess the token.
        Ok(BuildStatus::Continue)
    }
}
impl BuildFromTokens for TopLevelDocumentBuilder {
    // Don't error when someone tries to include a new file inside a block scope
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

    // This builder is responsible for spawning lower-level builders
    fn process_token(
        &mut self,
        py: Python,
        _py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        self.process_block_level_token(py, tok, data)
    }

    fn process_push_from_inner_builder(
        &mut self,
        py: Python,
        _py_env: &PyDict,
        pushed: Option<PushToNextLevel>,
        // closing_token: TTToken,
    ) -> TurnipTextContextlessResult<BuildStatus> {
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
                    self.structure.push_segment_header(py, header)?;
                    Ok(BuildStatus::Continue)
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
                    self.structure.push_to_topmost_block(py, p.as_ref(py))?;
                    Ok(BuildStatus::Continue)
                }
                DocElement::Block(block) => {
                    // Set expects_blank_line to *two* blank lines i.e. no content on the current line, and then a full line without content
                    // It's ok to set this flag high based on pushes from inner subfiles - it goes high when the subfile finishes anyway.
                    self.expects_n_blank_lines_after = Some((2, (elem_ctx, &block).into()));
                    self.structure.push_to_topmost_block(py, block.as_ref(py))?;
                    Ok(BuildStatus::Continue)
                }
                // If we get an inline, start building a paragraph with it
                DocElement::Inline(inline) => Ok(BuildStatus::StartInnerBuilder(
                    ParagraphFromTokens::new_with_inline(py, inline.as_ref(py), elem_ctx)?,
                )),
            },
            None => Ok(BuildStatus::Continue),
        }
    }
}

pub struct BlockScopeFromTokens {
    ctx: ParseContext,
    block_scope: Py<BlockScope>,
    /// If Some(), contains the span of a previously encountered token on this line that finished a block.
    /// New content is not allowed on the same line after finishing a block.
    expects_blank_line_after: Option<BlockModeElem>,
}
impl BlockScopeFromTokens {
    pub fn new_unowned(
        py: Python,
        first_tok: ParseSpan,
        last_tok: ParseSpan,
    ) -> TurnipTextContextlessResult<Self> {
        Ok(Self {
            ctx: ParseContext::new(first_tok, last_tok),
            block_scope: py_internal_alloc(py, BlockScope::new_empty(py))?,
            expects_blank_line_after: None,
        })
    }
}
impl BlockTokenProcessor for BlockScopeFromTokens {
    fn expects_new_line(&self) -> bool {
        self.expects_blank_line_after.is_some()
    }
    fn on_new_line_finish(&mut self) {
        self.expects_blank_line_after = None
    }
    fn on_unexpected_token_while_expecting_new_line(
        &mut self,
        tok: TTToken,
    ) -> TurnipTextContextlessError {
        InterpError::InsufficientBlockSeparation {
            last_block: std::mem::take(&mut self.expects_blank_line_after).expect(
                "This function is only called when self.expects_blank_line_after.is_some()",
            ),
            next_block_start: BlockModeElem::AnyToken(tok.token_span()),
        }
        .into()
    }

    fn on_close_scope(
        &mut self,
        py: Python,
        tok: TTToken,
        _data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        // This builder may receive tokens from inner files.
        // If it receives a token from an inner file, it returns an error.
        // This fulfils the contract for [BuildFromTokens::process_token].
        if !self.ctx.try_extend(&tok.token_span()) {
            // Closing block scope from different file
            // This must be a block-level scope close, because if an unbalanced scope close appeared in inline mode it would already have errored and not bubbled out.
            Err(InterpError::BlockScopeCloseOutsideScope(tok.token_span()).into())
        } else {
            Ok(BuildStatus::Done(Some((
                self.ctx,
                BlockElem::BlockScope(self.block_scope.clone_ref(py)).into(),
            ))))
        }
    }

    fn on_eof(&mut self, _py: Python, tok: TTToken) -> TurnipTextContextlessResult<BuildStatus> {
        Err(InterpError::EndedInsideScope {
            scope_start: self.ctx.first_tok(),
            eof_span: tok.token_span(),
        }
        .into())
    }
}
impl BuildFromTokens for BlockScopeFromTokens {
    // Don't error when someone tries to include a new file inside a block scope
    fn on_emitted_source_inside(
        &mut self,
        _code_emitting_source: ParseContext,
    ) -> TurnipTextContextlessResult<()> {
        // The tokens from this file will be passed through directly to us until we open new builders in its stack.
        // Allow the new file to start directly with content if it chooses.
        self.expects_blank_line_after = None;
        Ok(())
    }

    fn on_emitted_source_closed(&mut self, inner_source_emitted_by: ParseSpan) {
        // An inner file must have come from emitted code - a blank line must be seen before any new content after code emitting a file
        self.expects_blank_line_after =
            Some(BlockModeElem::SourceFromCode(inner_source_emitted_by));
    }

    fn process_token(
        &mut self,
        py: Python,
        _py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        self.process_block_level_token(py, tok, data)
    }

    fn process_push_from_inner_builder(
        &mut self,
        py: Python,
        _py_env: &PyDict,
        pushed: Option<PushToNextLevel>,
        // closing_token: TTToken,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        match pushed {
            Some((elem_ctx, elem)) => match elem {
                DocElement::HeaderFromCode(header) => {
                    Err(InterpError::CodeEmittedHeaderInBlockScope {
                        block_scope_start: self.ctx.first_tok(),
                        header,
                        code_span: elem_ctx.full_span(),
                    }
                    .into())
                }
                DocElement::Block(block) => {
                    // This must have been received from either
                    // 1. a paragraph, which has seen a blank line and ended itself
                    // 2. eval-brackets that directly emitted a block
                    // 3. eval-brackets that took some argument (a block scope, an inline scope, or a raw scope) and emitted a block
                    // 4. a manually opened block scope that was just closed
                    // In the case of 1, the paragraph will push the token for reprocessing so we can set expects_blank_line and it will immediately get unset.
                    // In the cases of 2, 3, and 4 we don't want any content between now and the end of the line, because that would be emitted into a new block and it would look confusing.
                    // Thus, set expects_blank_line.
                    // It's ok to set this flag high based on pushes from inner subfiles - it goes high when the subfile finishes anyway.
                    self.expects_blank_line_after = Some((elem_ctx, &block).into());
                    self.block_scope
                        .borrow_mut(py)
                        .push_block(block.as_ref(py))
                        .err_as_internal(py)?;
                    Ok(BuildStatus::Continue)
                }
                // If we get an inline, start building a paragraph inside this block-scope with it
                DocElement::Inline(inline) => Ok(BuildStatus::StartInnerBuilder(
                    ParagraphFromTokens::new_with_inline(py, inline.as_ref(py), elem_ctx)?,
                )),
            },
            None => Ok(BuildStatus::Continue),
        }
    }
}

/// This builder is initially started with a ScopeOpen token that may be a block scope open (followed by "\s*\n") or an inline scope open (followed by \s*[^\n]).
/// It starts out [BlockOrInlineScopeFromTokens::Undecided], then based on the following tokens either decides on [BlockOrInlineScopeFromTokens::Block] or [BlockOrInlineScopeFromTokens::Inline] and from then on acts as exactly [BlockScopeFromTokens] or [InlineScopeFromTokens] respectfully.
pub enum BlockLevelAmbiguousScope {
    Undecided { first_tok: ParseSpan },
    Block(BlockScopeFromTokens),
    Inline(KnownInlineScopeFromTokens),
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
                    let block_builder =
                        BlockScopeFromTokens::new_unowned(py, *first_tok, last_tok)?;
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
                    let mut inline_builder = KnownInlineScopeFromTokens::new_unowned(
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
