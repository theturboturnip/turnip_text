use std::{cell::RefCell, rc::Rc};

use pyo3::{prelude::*, types::PyDict};

use crate::{
    error::{
        interp::{BlockModeElem, InlineModeContext, InterpError, MapContextlessResult},
        TurnipTextContextlessResult,
    },
    interpreter::state_machines::{BlockElem, InlineElem},
    lexer::{Escapable, TTToken},
    python::interop::{InlineScope, Paragraph, Raw, Sentence, Text},
    util::{ParseContext, ParseSpan},
};

use super::{
    code::CodeFromTokens, comment::CommentFromTokens, py_internal_alloc, rc_refcell,
    BuildFromTokens, BuildStatus, DocElement, PushToNextLevel,
};

struct InlineTextState {
    text: String,
    /// pending_whitespace is appended to `text` before new text is added, but can be ignored in certain scenarios.
    ///
    /// e.g. "the" + Whitespace(" ") => ("the", " ") - when the next token is "apple", becomes "the" + " " + "apple"
    /// but for "the" + Whitespace(" ") + Newline, the pending_whitespace is dropped.
    pending_whitespace: Option<String>,
}
impl InlineTextState {
    fn new() -> Self {
        Self {
            text: String::new(),
            pending_whitespace: None,
        }
    }

    fn new_with_text(text: &str) -> Self {
        Self {
            text: text.to_string(),
            pending_whitespace: None,
        }
    }

    fn clear_pending_whitespace(&mut self) {
        self.pending_whitespace = None;
    }

    fn encounter_text(&mut self, text_content: &str) {
        if let Some(w) = std::mem::take(&mut self.pending_whitespace) {
            self.text.push_str(&w)
        }
        self.text.push_str(text_content);
    }

    fn encounter_whitespace(&mut self, whitespace_content: &str) {
        match &mut self.pending_whitespace {
            Some(w) => w.push_str(whitespace_content),
            None => self.pending_whitespace = Some(whitespace_content.to_string()),
        };
    }

    /// Take the text component (optionally including the pending whitespace), and put it into a Text() inline object if non-empty.
    fn consume(
        &mut self,
        py: Python,
        include_whitespace: bool,
    ) -> TurnipTextContextlessResult<Option<Py<Text>>> {
        if let Some(w) = std::mem::take(&mut self.pending_whitespace) {
            if include_whitespace {
                self.text.push_str(&w)
            }
        }
        if !self.text.is_empty() {
            let old_text = std::mem::replace(&mut self.text, String::new());
            Ok(Some(py_internal_alloc(py, Text::new_rs(py, &old_text))?))
        } else {
            Ok(None)
        }
    }
}

trait InlineTokenProcessor {
    fn inline_mode_ctx(&self) -> InlineModeContext;

    fn ignore_whitespace(&self) -> bool;
    fn clear_pending_whitespace(&mut self);
    fn flush_pending_text(
        &mut self,
        py: Python,
        include_whitespace: bool,
    ) -> TurnipTextContextlessResult<()>;

    fn on_plain_text(
        &mut self,
        py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus>;
    fn on_midline_whitespace(
        &mut self,
        py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus>;
    fn on_newline(&mut self, py: Python, tok: TTToken) -> TurnipTextContextlessResult<BuildStatus>;
    fn on_open_scope(
        &mut self,
        py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus>;
    fn on_close_scope(
        &mut self,
        py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus>;
    fn on_eof(&mut self, py: Python, tok: TTToken) -> TurnipTextContextlessResult<BuildStatus>;

    fn process_inline_level_token(
        &mut self,
        py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        match tok {
            // Escaped newline => "Continue sentence"
            TTToken::Escaped(_, Escapable::Newline) => Ok(BuildStatus::Continue),
            TTToken::Escaped(_, _) | TTToken::Backslash(_) | TTToken::OtherText(_) => {
                self.on_plain_text(py, tok, data)
            }
            TTToken::Whitespace(_) => {
                if self.ignore_whitespace() {
                    Ok(BuildStatus::Continue)
                } else {
                    self.on_midline_whitespace(py, tok, data)
                }
            }
            TTToken::Newline(_) => {
                self.clear_pending_whitespace();
                self.flush_pending_text(py, true)?;
                self.on_newline(py, tok)
            }
            TTToken::EOF(_) => {
                self.clear_pending_whitespace();
                self.flush_pending_text(py, true)?;
                self.on_eof(py, tok)
            }
            TTToken::ScopeOpen(_) => {
                self.flush_pending_text(py, true)?;
                self.on_open_scope(py, tok, data)
            }
            TTToken::ScopeClose(_) => {
                self.clear_pending_whitespace();
                self.flush_pending_text(py, true)?;
                self.on_close_scope(py, tok, data)
            }
            TTToken::Hashes(_, _) => {
                self.clear_pending_whitespace();
                self.flush_pending_text(py, true)?;
                Ok(BuildStatus::StartInnerBuilder(CommentFromTokens::new()))
            }

            // Note this may return Block
            TTToken::CodeOpen(start_span, n_brackets) => {
                self.flush_pending_text(py, true)?;
                Ok(BuildStatus::StartInnerBuilder(CodeFromTokens::new(
                    start_span, n_brackets,
                )))
            }

            TTToken::RawScopeOpen(start_span, n_opening) => {
                self.flush_pending_text(py, true)?;
                Ok(BuildStatus::StartInnerBuilder(RawStringFromTokens::new(
                    start_span, n_opening,
                )))
            }

            TTToken::CodeClose(span, _) => Err(InterpError::CodeCloseOutsideCode(span).into()),

            TTToken::RawScopeClose(span, _) => {
                Err(InterpError::RawScopeCloseOutsideRawScope(span).into())
            }
        }
    }
}

pub struct ParagraphFromTokens {
    ctx: ParseContext,
    para: Py<Paragraph>,
    start_of_line: bool,
    current_building_text: InlineTextState,
    current_sentence: Py<Sentence>,
}
impl ParagraphFromTokens {
    pub fn new_with_inline(
        py: Python,
        inline: &PyAny,
        inline_ctx: ParseContext,
    ) -> TurnipTextContextlessResult<Rc<RefCell<Self>>> {
        let current_sentence = py_internal_alloc(py, Sentence::new_empty(py))?;
        current_sentence
            .borrow_mut(py)
            .push_inline(inline)
            .err_as_internal(py)?;
        Ok(rc_refcell(Self {
            ctx: ParseContext::new(inline_ctx.first_tok(), inline_ctx.last_tok()),
            para: py_internal_alloc(py, Paragraph::new_empty(py))?,
            start_of_line: false,
            current_building_text: InlineTextState::new(),
            current_sentence,
        }))
    }
    pub fn new_with_starting_text(
        py: Python,
        text: &str,
        text_span: ParseSpan,
    ) -> TurnipTextContextlessResult<Rc<RefCell<Self>>> {
        Ok(rc_refcell(Self {
            ctx: ParseContext::new(text_span, text_span),
            para: py_internal_alloc(py, Paragraph::new_empty(py))?,
            start_of_line: false,
            current_building_text: InlineTextState::new_with_text(text),
            current_sentence: py_internal_alloc(py, Sentence::new_empty(py))?,
        }))
    }
    fn fold_current_text_into_sentence(
        &mut self,
        py: Python,
        include_whitespace: bool,
    ) -> TurnipTextContextlessResult<()> {
        let py_text = self.current_building_text.consume(py, include_whitespace)?;
        if let Some(py_text) = py_text {
            self.current_sentence
                .borrow_mut(py)
                .push_inline(py_text.as_ref(py))
                .err_as_internal(py)
        } else {
            Ok(())
        }
    }
    fn fold_current_sentence_into_paragraph(
        &mut self,
        py: Python,
    ) -> TurnipTextContextlessResult<()> {
        // If the sentence is empty, don't bother pushing
        if self.current_sentence.borrow(py).__len__(py) > 0 {
            // Swap the current sentence out for a new one
            let sentence = std::mem::replace(
                &mut self.current_sentence,
                py_internal_alloc(py, Sentence::new_empty(py))?,
            );
            // Push the old one into the paragraph
            self.para
                .borrow_mut(py)
                .push_sentence(sentence.as_ref(py))
                .err_as_internal(py)?;
        }
        Ok(())
    }
}
impl InlineTokenProcessor for ParagraphFromTokens {
    fn inline_mode_ctx(&self) -> InlineModeContext {
        InlineModeContext::Paragraph(self.ctx)
    }

    fn ignore_whitespace(&self) -> bool {
        // Swallow whitespace at the start of the line
        self.start_of_line
    }
    fn clear_pending_whitespace(&mut self) {
        self.current_building_text.clear_pending_whitespace()
    }
    fn flush_pending_text(
        &mut self,
        py: Python,
        include_whitespace: bool,
    ) -> TurnipTextContextlessResult<()> {
        self.fold_current_text_into_sentence(py, include_whitespace)
    }

    fn on_plain_text(
        &mut self,
        _py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        self.start_of_line = false;
        self.current_building_text
            .encounter_text(tok.stringify_escaped(data));
        assert!(
            self.ctx.try_extend(&tok.token_span()),
            "ParagraphFromTokens got a token from a different file that it was opened in"
        );

        Ok(BuildStatus::Continue)
    }

    fn on_midline_whitespace(
        &mut self,
        _py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        self.current_building_text
            .encounter_whitespace(tok.stringify_escaped(data));
        assert!(
            self.ctx.try_extend(&tok.token_span()),
            "ParagraphFromTokens got a token from a different file that it was opened in"
        );

        Ok(BuildStatus::Continue)
    }

    fn on_newline(&mut self, py: Python, tok: TTToken) -> TurnipTextContextlessResult<BuildStatus> {
        // Always extend our token span so error messages using it have the full context
        assert!(
            self.ctx.try_extend(&tok.token_span()),
            "ParagraphFromTokens got a token from a different file that it was opened in"
        );
        // Text is already folded into the sentence
        if self.start_of_line {
            Ok(BuildStatus::DoneAndReprocessToken(Some((
                self.ctx,
                BlockElem::Para(self.para.clone_ref(py)).into(),
            ))))
        } else {
            self.fold_current_sentence_into_paragraph(py)?;
            // We're now at the start of the line
            self.start_of_line = true;
            Ok(BuildStatus::Continue)
        }
    }

    fn on_eof(&mut self, py: Python, tok: TTToken) -> TurnipTextContextlessResult<BuildStatus> {
        assert!(
            self.ctx.try_extend(&tok.token_span()),
            "ParagraphFromTokens got a token from a different file that it was opened in"
        );
        if !self.start_of_line {
            self.fold_current_sentence_into_paragraph(py)?;
        }
        Ok(BuildStatus::DoneAndReprocessToken(Some((
            self.ctx,
            BlockElem::Para(self.para.clone_ref(py)).into(),
        ))))
    }

    fn on_open_scope(
        &mut self,
        _py: Python,
        tok: TTToken,
        _data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        Ok(BuildStatus::StartInnerBuilder(
            InlineLevelAmbiguousScope::new(
                InlineModeContext::Paragraph(self.ctx),
                self.start_of_line,
                tok.token_span(),
            ),
        ))
    }

    fn on_close_scope(
        &mut self,
        py: Python,
        tok: TTToken,
        _data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        // If the closing brace is at the start of the line, it must be for block-scope and we can assume there won't be text afterwards.
        // End the paragraph, and tell the scope above us in the hierarchy to handle the scope close.
        if self.start_of_line {
            assert!(
                self.ctx.try_extend(&tok.token_span()),
                "ParagraphFromTokens got a token from a different file that it was opened in"
            );
            Ok(BuildStatus::DoneAndReprocessToken(Some((
                self.ctx,
                BlockElem::Para(self.para.clone_ref(py)).into(),
            ))))
        } else {
            Err(InterpError::InlineScopeCloseOutsideScope(tok.token_span()).into())
        }
    }
}
impl BuildFromTokens for ParagraphFromTokens {
    fn process_token(
        &mut self,
        py: Python,
        _py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        self.process_inline_level_token(py, tok, data)
    }

    fn process_push_from_inner_builder(
        &mut self,
        py: Python,
        _py_env: &PyDict,
        pushed: Option<PushToNextLevel>,
        // closing_token: TTToken,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        self.start_of_line = false;
        match pushed {
            Some((elem_ctx, elem)) => match elem {
                // Can't get a header or a block in an inline scope
                DocElement::HeaderFromCode(header) => {
                    return Err(InterpError::CodeEmittedHeaderInInlineMode {
                        inl_mode: self.inline_mode_ctx(),
                        header,
                        code_span: elem_ctx.full_span(),
                    }
                    .into())
                }
                DocElement::Block(BlockElem::FromCode(block)) => {
                    // This must have come from code.
                    if self.start_of_line {
                        // Someone is trying to open a new, separate block and not a block "inside" the paragraph.
                        // Give them a more relevant error message.
                        return Err(InterpError::InsufficientBlockSeparation {
                            last_block: BlockModeElem::Para(self.ctx),
                            next_block_start: BlockModeElem::BlockFromCode(elem_ctx.full_span()),
                        }
                        .into());
                    }
                    // We're deep inside a paragraph here.
                    return Err(InterpError::CodeEmittedBlockInInlineMode {
                        inl_mode: self.inline_mode_ctx(),
                        block,
                        code_span: elem_ctx.full_span(),
                    }
                    .into());
                }
                DocElement::Block(BlockElem::BlockScope(_)) => {
                    unreachable!("ParagraphFromTokens never tries to build a BlockScope")
                }
                DocElement::Block(BlockElem::Para(_)) => {
                    unreachable!("ParagraphFromTokens never tries to build an inner Paragraph")
                }
                // If we get an inline, shove it in
                DocElement::Inline(inline) => {
                    assert!(self.ctx.try_combine(elem_ctx),
                        "ParagraphFromTokens got a token from a different file that it was opened in"
                    );

                    self.current_sentence
                        .borrow_mut(py)
                        .push_inline(inline.as_ref(py))
                        .err_as_internal(py)?;
                }
            },
            None => {}
        }
        Ok(BuildStatus::Continue)
    }

    fn on_emitted_source_inside(
        &mut self,
        code_emitting_source: ParseContext,
    ) -> TurnipTextContextlessResult<()> {
        if self.start_of_line {
            // Someone is trying to open a new file separately from the paragraph and not "inside" the paragraph.
            // Give them a more relevant error message.
            Err(InterpError::InsufficientBlockSeparation {
                last_block: BlockModeElem::Para(self.ctx),
                next_block_start: BlockModeElem::SourceFromCode(code_emitting_source.full_span()),
            }
            .into())
        } else {
            Err(InterpError::CodeEmittedSourceInInlineMode {
                inl_mode: self.inline_mode_ctx(),
                code_span: code_emitting_source.full_span(),
            }
            .into())
        }
    }

    fn on_emitted_source_closed(&mut self, _inner_source_emitted_by: ParseSpan) {
        unreachable!("ParagraphFromTokens always returns Err on_emitted_source_inside")
    }
}

/// Parser for a scope which based on context *should* be inline, i.e. if you encounter no content before a newline then you must throw an error.
pub enum InlineLevelAmbiguousScope {
    Undecided {
        preceding_inline: InlineModeContext,
        start_of_line: bool,
        scope_ctx: ParseContext,
    },
    Known(KnownInlineScopeFromTokens),
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
                    let mut inline_builder = KnownInlineScopeFromTokens::new_unowned(
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

/// Parser for a scope that is known to be an inline scope, i.e. has content on the same line as the scope open.
pub struct KnownInlineScopeFromTokens {
    preceding_inline: Option<InlineModeContext>,
    ctx: ParseContext,
    inline_scope: Py<InlineScope>,
    start_of_scope: bool,
    current_building_text: InlineTextState,
}
impl KnownInlineScopeFromTokens {
    pub fn new_unowned(
        py: Python,
        preceding_inline: Option<InlineModeContext>,
        ctx: ParseContext,
    ) -> TurnipTextContextlessResult<Self> {
        Ok(Self {
            preceding_inline,
            ctx,
            start_of_scope: true,
            inline_scope: py_internal_alloc(py, InlineScope::new_empty(py))?,
            current_building_text: InlineTextState::new(),
        })
    }

    /// Replace self.current_building_text with None. If it was Some() before, take the text component (not the pending whitespace) put it into a Text() inline object, and push that object into the inline scope.
    fn fold_current_text_into_scope(
        &mut self,
        py: Python,
        include_whitespace: bool,
    ) -> TurnipTextContextlessResult<()> {
        let py_text = self.current_building_text.consume(py, include_whitespace)?;
        if let Some(py_text) = py_text {
            self.inline_scope
                .borrow_mut(py)
                .push_inline(py_text.as_ref(py))
                .err_as_internal(py)
        } else {
            Ok(())
        }
    }
}
impl InlineTokenProcessor for KnownInlineScopeFromTokens {
    fn inline_mode_ctx(&self) -> InlineModeContext {
        InlineModeContext::InlineScope {
            scope_start: self.ctx.first_tok(),
        }
    }

    fn ignore_whitespace(&self) -> bool {
        // Swallow whitespace at the start of the scope
        self.start_of_scope
    }
    fn clear_pending_whitespace(&mut self) {
        self.current_building_text.clear_pending_whitespace();
    }
    fn flush_pending_text(
        &mut self,
        py: Python,
        include_whitespace: bool,
    ) -> TurnipTextContextlessResult<()> {
        self.fold_current_text_into_scope(py, include_whitespace)
    }

    fn on_plain_text(
        &mut self,
        _py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        self.start_of_scope = false;
        self.current_building_text
            .encounter_text(tok.stringify_escaped(data));

        Ok(BuildStatus::Continue)
    }

    fn on_midline_whitespace(
        &mut self,
        _py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        self.current_building_text
            .encounter_whitespace(tok.stringify_escaped(data));

        Ok(BuildStatus::Continue)
    }

    fn on_newline(&mut self, py: Python, tok: TTToken) -> TurnipTextContextlessResult<BuildStatus> {
        if self.inline_scope.borrow_mut(py).__len__(py) == 0 {
            unreachable!("KnownInlineScopeFromTokens received a newline with no preceding content - was actually a block scope");
        } else {
            // If there was content then we were definitely interrupted in the middle of a sentence
            Err(InterpError::SentenceBreakInInlineScope {
                scope_start: self.ctx.first_tok(),
                sentence_break: tok.token_span(),
            }
            .into())
        }
    }

    // TODO test error reporting for nested inline scopes
    fn on_open_scope(
        &mut self,
        _py: Python,
        tok: TTToken,
        _data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        let new_scopes_inline_context = match self.preceding_inline {
            // If we're part of a paragraph, the inner scope is part of a paragraph too
            // Just extend the paragraph to include the current token
            // TODO test this gets all the tokens up to but not including the inline scope
            Some(InlineModeContext::Paragraph(preceding_para)) => {
                let mut new = preceding_para.clone();
                assert!(new.try_combine(self.ctx), "Paragraph, KnownInlineScopeFromTokens don't generate source files, must always receive tokens from the same file");
                InlineModeContext::Paragraph(new)
            }
            // If we aren't part of a paragraph, say the inner builder is in inline mode because of us
            None | Some(InlineModeContext::InlineScope { .. }) => InlineModeContext::InlineScope {
                scope_start: self.ctx.first_tok(),
            },
        };
        Ok(BuildStatus::StartInnerBuilder(
            InlineLevelAmbiguousScope::new(new_scopes_inline_context, false, tok.token_span()),
        ))
    }
    fn on_close_scope(
        &mut self,
        py: Python,
        tok: TTToken,
        _data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        // pending text has already been folded in
        assert!(
            self.ctx.try_extend(&tok.token_span()),
            "InlineScopeFromTokens got a token from a different file that it was opened in"
        );
        Ok(BuildStatus::Done(Some((
            self.ctx,
            InlineElem::InlineScope(self.inline_scope.clone_ref(py)).into(),
        ))))
    }

    fn on_eof(&mut self, _py: Python, tok: TTToken) -> TurnipTextContextlessResult<BuildStatus> {
        Err(InterpError::EndedInsideScope {
            scope_start: self.ctx.first_tok(),
            eof_span: tok.token_span(),
        }
        .into())
    }
}
impl BuildFromTokens for KnownInlineScopeFromTokens {
    fn process_token(
        &mut self,
        py: Python,
        _py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        self.process_inline_level_token(py, tok, data)
    }

    fn process_push_from_inner_builder(
        &mut self,
        py: Python,
        _py_env: &PyDict,
        pushed: Option<PushToNextLevel>,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        self.start_of_scope = false;
        // Before we do anything else, push the current text into the scope including the whitespace between the text and the newly pushed item
        self.fold_current_text_into_scope(py, true)?;
        match pushed {
            Some((elem_ctx, elem)) => match elem {
                // Can't get a header or a block in an inline scope
                DocElement::HeaderFromCode(header) => {
                    return Err(InterpError::CodeEmittedHeaderInInlineMode {
                        inl_mode: self.inline_mode_ctx(),
                        header,
                        code_span: elem_ctx.full_span(),
                    }
                    .into())
                }
                DocElement::Block(BlockElem::FromCode(block)) => {
                    return Err(InterpError::CodeEmittedBlockInInlineMode {
                        inl_mode: self.inline_mode_ctx(),
                        block,
                        code_span: elem_ctx.full_span(),
                    }
                    .into())
                }
                DocElement::Block(BlockElem::BlockScope(_)) => {
                    unreachable!("InlineScopeFromTokens never tries to build a BlockScope")
                }
                DocElement::Block(BlockElem::Para(_)) => {
                    unreachable!("InlineScopeFromTokens never tries to build an inner Paragraph")
                }
                // If we get an inline, shove it in
                DocElement::Inline(inline) => {
                    self.inline_scope
                        .borrow_mut(py)
                        .push_inline(inline.as_ref(py))
                        .err_as_internal(py)?;
                }
            },
            None => {}
        };
        Ok(BuildStatus::Continue)
    }

    fn on_emitted_source_inside(
        &mut self,
        code_emitting_source: ParseContext,
    ) -> TurnipTextContextlessResult<()> {
        Err(InterpError::CodeEmittedSourceInInlineMode {
            inl_mode: self.inline_mode_ctx(),
            code_span: code_emitting_source.full_span(),
        }
        .into())
    }

    fn on_emitted_source_closed(&mut self, _inner_source_emitted_by: ParseSpan) {}
}

pub struct RawStringFromTokens {
    ctx: ParseContext,
    n_closing: usize,
    raw_data: String,
}
impl RawStringFromTokens {
    pub fn new(start_span: ParseSpan, n_opening: usize) -> Rc<RefCell<Self>> {
        rc_refcell(Self {
            ctx: ParseContext::new(start_span, start_span),
            n_closing: n_opening,
            raw_data: String::new(),
        })
    }
}
impl BuildFromTokens for RawStringFromTokens {
    fn process_token(
        &mut self,
        py: Python,
        _py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        // This builder does not directly emit new source files, so it cannot receive tokens from inner files.
        // When receiving EOF it returns an error.
        // This fulfils the contract for [BuildFromTokens::process_token].
        match tok {
            TTToken::RawScopeClose(_, given_closing) if given_closing == self.n_closing => {
                self.ctx.try_extend(&tok.token_span());
                let raw = py_internal_alloc(
                    py,
                    Raw::new_rs(py, std::mem::take(&mut self.raw_data).as_str()),
                )?;
                Ok(BuildStatus::Done(Some((
                    self.ctx,
                    InlineElem::Raw(raw).into(),
                ))))
            }
            TTToken::EOF(eof_span) => Err(InterpError::EndedInsideRawScope {
                raw_scope_start: self.ctx.first_tok(),
                eof_span,
            }
            .into()),
            _ => {
                self.raw_data.push_str(tok.stringify_raw(data));
                Ok(BuildStatus::Continue)
            }
        }
    }

    fn process_push_from_inner_builder(
        &mut self,
        _py: Python,
        _py_env: &PyDict,
        _pushed: Option<PushToNextLevel>,
        // closing_token: TTToken,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        panic!("RawStringFromTokens does not spawn inner builders")
    }

    fn on_emitted_source_inside(
        &mut self,
        _code_emitting_source: ParseContext,
    ) -> TurnipTextContextlessResult<()> {
        unreachable!("RawStringFromTokens does not spawn an inner code builder, so cannot have a source file emitted inside")
    }
    fn on_emitted_source_closed(&mut self, _inner_source_emitted_by: ParseSpan) {
        unreachable!("RawStringFromTokens does not spawn an inner code builder, so cannot have a source file emitted inside")
    }
}
