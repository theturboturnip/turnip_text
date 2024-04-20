use std::{cell::RefCell, rc::Rc};

use pyo3::{prelude::*, types::PyDict};

use crate::{
    error::{
        interp::{InterpError, MapContextlessResult},
        TurnipTextContextlessResult,
    },
    interpreter::state_machines::{BlockElem, InlineElem},
    lexer::{Escapable, TTToken},
    python::interop::{InlineScope, Paragraph, Raw, Sentence, Text},
    util::ParseSpan,
};

use super::{
    code::CodeFromTokens, comment::CommentFromTokens, py_internal_alloc, rc_refcell,
    BuildFromTokens, BuildStatus, DocElement, ParseContext, PushToNextLevel,
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
            ctx: ParseContext::new(inline_ctx.first_tok, inline_ctx.last_tok),
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
        py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        self.start_of_line = false;
        self.current_building_text
            .encounter_text(tok.stringify_escaped(data));

        Ok(BuildStatus::Continue)
    }

    fn on_midline_whitespace(
        &mut self,
        py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        self.current_building_text
            .encounter_whitespace(tok.stringify_escaped(data));

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
            Ok(BuildStatus::DoneAndReprocessToken(Some(
                self.ctx
                    .make(BlockElem::Para(self.para.clone_ref(py)).into()),
            )))
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
        Ok(BuildStatus::DoneAndReprocessToken(Some(
            self.ctx
                .make(BlockElem::Para(self.para.clone_ref(py)).into()),
        )))
    }

    fn on_open_scope(
        &mut self,
        py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        Ok(BuildStatus::StartInnerBuilder(InlineScopeFromTokens::new(
            py,
            tok.token_span(),
            if self.start_of_line {
                AmbiguousInlineContext::StartOfLineMidPara {
                    para_in_progress: self.ctx,
                }
            } else {
                AmbiguousInlineContext::UnambiguouslyInline
            },
        )?))
    }

    fn on_close_scope(
        &mut self,
        py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        // If the closing brace is at the start of the line, it must be for block-scope and we can assume there won't be text afterwards.
        // End the paragraph, and tell the scope above us in the hierarchy to handle the scope close.
        if self.start_of_line {
            assert!(
                self.ctx.try_extend(&tok.token_span()),
                "ParagraphFromTokens got a token from a different file that it was opened in"
            );
            Ok(BuildStatus::DoneAndReprocessToken(Some(
                self.ctx
                    .make(BlockElem::Para(self.para.clone_ref(py)).into()),
            )))
        } else {
            Err(InterpError::InlineScopeCloseOutsideScope(tok.token_span()).into())
        }
    }
}
impl BuildFromTokens for ParagraphFromTokens {
    fn process_token(
        &mut self,
        py: Python,
        py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        self.process_inline_level_token(py, tok, data)
    }

    fn process_push_from_inner_builder(
        &mut self,
        py: Python,
        py_env: &PyDict,
        pushed: Option<PushToNextLevel>,
        // closing_token: TTToken,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        self.start_of_line = false;
        match pushed {
            Some(PushToNextLevel { from_builder, elem }) => match elem {
                // Can't get a header or a block in an inline scope
                DocElement::HeaderFromCode(_) => {
                    return Err(InterpError::DocSegmentHeaderMidPara {
                        code_span: from_builder.full_span(),
                    }
                    .into())
                }
                DocElement::Block(_) => {
                    // This must have come from code.
                    if self.start_of_line {
                        // Someone is trying to open a new, separate block and not a block "inside" the paragraph.
                        // Give them a more relevant error message.
                        return Err(InterpError::InsufficientParaNewBlockOrFileSeparation {
                            para: self.ctx.full_span(), // TODO should prob give the first and last token separately "paragraph started here...", "paragraph was still in progress here..."
                            next_block_start: from_builder.full_span(),
                            was_block_not_file: true,
                        }
                        .into());
                    }
                    // We're deep inside a paragraph here.
                    return Err(InterpError::BlockCodeMidPara {
                        code_span: from_builder.full_span(),
                    }
                    .into());
                }
                // If we get an inline, shove it in
                DocElement::Inline(inline) => {
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
        from_builder: ParseContext,
    ) -> TurnipTextContextlessResult<()> {
        if self.start_of_line {
            // Someone is trying to open a new file separately from the paragraph and not "inside" the paragraph.
            // Give them a more relevant error message.
            Err(InterpError::InsufficientParaNewBlockOrFileSeparation {
                para: self.ctx.full_span(),
                next_block_start: from_builder.full_span(),
                was_block_not_file: false,
            }
            .into())
        } else {
            Err(InterpError::InsertedFileMidPara {
                code_span: from_builder.full_span(),
            }
            .into())
        }
    }
}

pub enum AmbiguousInlineContext {
    StartOfLineMidPara { para_in_progress: ParseContext },
    UnambiguouslyInline,
}

pub struct InlineScopeFromTokens {
    ctx: ParseContext,
    inline_scope: Py<InlineScope>,
    start_of_scope: bool,
    current_building_text: InlineTextState,
    ambiguous_inline_context: AmbiguousInlineContext,
}
impl InlineScopeFromTokens {
    /// Create a new InlineScope builder.
    ///
    /// Set `ambiguous_inline_context` to [AmbiguousInlineContext::StartOfLine] if we're at the start of a line, because the user could be correctly continuing the paragraph or incorrectly try to create a new block immediately.
    /// If a newline is encountered before any non-whitespace content, the user has effectively tried to open a block scope.
    /// If we're in an ambiguous inline context, the error raised will be [InterpError::InsufficientBlockSeparation] to encourage the user to open the block on a new line.
    /// In all other cases, the error will be [InterpError::BlockScopeOpenedMidPara] because it's clear that's what they actually were trying to do.
    pub fn new(
        py: Python,
        start_span: ParseSpan,
        ambiguous_inline_context: AmbiguousInlineContext,
    ) -> TurnipTextContextlessResult<Rc<RefCell<Self>>> {
        Ok(rc_refcell(Self::new_unowned(
            py,
            start_span,
            ambiguous_inline_context,
        )?))
    }

    pub fn new_unowned(
        py: Python,
        start_span: ParseSpan,
        ambiguous_inline_context: AmbiguousInlineContext,
    ) -> TurnipTextContextlessResult<Self> {
        Ok(Self {
            ctx: ParseContext::new(start_span, start_span),
            start_of_scope: true,
            inline_scope: py_internal_alloc(py, InlineScope::new_empty(py))?,
            current_building_text: InlineTextState::new(),
            ambiguous_inline_context,
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
impl InlineTokenProcessor for InlineScopeFromTokens {
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
        py: Python,
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
        py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        self.current_building_text
            .encounter_whitespace(tok.stringify_escaped(data));

        Ok(BuildStatus::Continue)
    }

    fn on_newline(&mut self, py: Python, tok: TTToken) -> TurnipTextContextlessResult<BuildStatus> {
        // If there is only whitespace between us and the newline, this is actually a block scope open.
        // Clearly we're supposed to be in inline mode - InlineScopeFromTokens is only created inside
        // - BlockOrInlineScopeFromTokens
        // - anything implementing InlineTokenProcessor i.e.
        //   - ParagraphFromTokens
        //   - another instance of InlineScopeFromTokens
        //
        // in the BlockOrInlineScopeFromTokens case, the newline would have been used to resolve ambiguity and it would be building a block scope - this function wouldn't have been called.
        // => if we encounter just whitespace and a newline, we must in inside ParagraphFromTokens or InlineScopeFromTokens
        // => i.e. we must be in inline mode
        // => and the user has just tried to open a block scope i.e. r"{\s*\n"
        // => Error: Block Scope Opened In Inline Mode
        // (InlineTokenProcessor automatically flushes our content before calling on_newline(), so we don't need to check self.current_building_text).
        if self.inline_scope.borrow_mut(py).__len__(py) == 0 {
            match self.ambiguous_inline_context {
                AmbiguousInlineContext::StartOfLineMidPara { para_in_progress } => {
                    Err(InterpError::InsufficientParaNewBlockOrFileSeparation {
                        para: para_in_progress.full_span(),
                        next_block_start: self.ctx.full_span(),
                        was_block_not_file: true,
                    }
                    .into())
                }
                AmbiguousInlineContext::UnambiguouslyInline => {
                    Err(InterpError::BlockScopeOpenedMidPara {
                        scope_start: self.ctx.first_tok,
                    }
                    .into())
                }
            }
        } else {
            // If there was content then we were definitely interrupted in the middle of a sentence
            Err(InterpError::SentenceBreakInInlineScope {
                scope_start: self.ctx.first_tok,
            }
            .into())
        }
    }

    fn on_open_scope(
        &mut self,
        py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        Ok(BuildStatus::StartInnerBuilder(InlineScopeFromTokens::new(
            py,
            tok.token_span(),
            AmbiguousInlineContext::UnambiguouslyInline,
        )?))
    }
    fn on_close_scope(
        &mut self,
        py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        // pending text has already been folded in
        assert!(
            self.ctx.try_extend(&tok.token_span()),
            "InlineScopeFromTokens got a token from a different file that it was opened in"
        );
        Ok(BuildStatus::Done(Some(self.ctx.make(
            InlineElem::InlineScope(self.inline_scope.clone_ref(py)).into(),
        ))))
    }

    fn on_eof(&mut self, py: Python, tok: TTToken) -> TurnipTextContextlessResult<BuildStatus> {
        Err(InterpError::EndedInsideScope {
            scope_start: self.ctx.first_tok,
        }
        .into())
    }
}
impl BuildFromTokens for InlineScopeFromTokens {
    fn process_token(
        &mut self,
        py: Python,
        py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        self.process_inline_level_token(py, tok, data)
    }

    fn process_push_from_inner_builder(
        &mut self,
        py: Python,
        py_env: &PyDict,
        pushed: Option<PushToNextLevel>,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        self.start_of_scope = false;
        // Before we do anything else, push the current text into the scope including the whitespace between the text and the newly pushed item
        self.fold_current_text_into_scope(py, true)?;
        match pushed {
            Some(PushToNextLevel { from_builder, elem }) => match elem {
                // Can't get a header or a block in an inline scope
                DocElement::HeaderFromCode(_) => {
                    return Err(InterpError::DocSegmentHeaderMidPara {
                        code_span: from_builder.full_span(),
                    }
                    .into())
                }
                DocElement::Block(_) => {
                    return Err(InterpError::BlockCodeMidPara {
                        code_span: from_builder.full_span(),
                    }
                    .into())
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
        py_env: &PyDict,
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
                Ok(BuildStatus::Done(Some(
                    self.ctx.make(InlineElem::Raw(raw).into()),
                )))
            }
            TTToken::EOF(_) => Err(InterpError::EndedInsideRawScope {
                raw_scope_start: self.ctx.first_tok,
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
        py: Python,
        py_env: &PyDict,
        pushed: Option<PushToNextLevel>,
        // closing_token: TTToken,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        panic!("RawStringFromTokens does not spawn inner builders")
    }
}
