use pyo3::{prelude::*, types::PyDict};

use crate::{
    error::{
        interp::{BlockModeElem, InlineModeContext, InterpError, MapContextlessResult},
        TurnipTextContextlessError, TurnipTextContextlessResult,
    },
    interpreter::state_machines::{BlockElem, InlineElem},
    lexer::{Escapable, TTToken},
    python::{
        interop::{Block, DocSegmentHeader, InlineScope, Paragraph, Raw, Sentence, Text},
        typeclass::PyTcRef,
    },
    util::{ParseContext, ParseSpan},
};

use super::{
    ambiguous_scope::InlineLevelAmbiguousScopeProcessor, code::CodeProcessor,
    comment::CommentProcessor, py_internal_alloc, rc_refcell, DocElement, EmittedElement,
    ProcStatus, TokenProcessor,
};

// Only expose specific implementations of InlineLevelProcessor
pub type ParagraphProcessor = InlineLevelProcessor<ParagraphInlineMode>;
pub type KnownInlineScopeProcessor = InlineLevelProcessor<KnownInlineScopeInlineMode>;

/// This struct handled inline-mode processing.
///
/// See [BlockLevelProcessor] for an explanation of this design pattern.
pub struct InlineLevelProcessor<T> {
    inner: T,
    current_building_text: InlineTextState,
}

/// This trait overrides behaviour of the InlineLevelProcessor in specific cases.
trait InlineMode {
    fn inline_mode_ctx(&self) -> InlineModeContext;
    fn ignore_whitespace(&self) -> bool;

    fn on_content(&mut self);

    fn on_newline(&mut self, py: Python, tok: TTToken) -> TurnipTextContextlessResult<ProcStatus>;
    fn on_open_scope(
        &mut self,
        py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<ProcStatus>;
    fn on_close_scope(
        &mut self,
        py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<ProcStatus>;
    fn on_eof(&mut self, py: Python, tok: TTToken) -> TurnipTextContextlessResult<ProcStatus>;

    fn err_on_header_from_code(
        &self,
        header: PyTcRef<DocSegmentHeader>,
        header_ctx: ParseContext,
    ) -> TurnipTextContextlessError;
    fn err_on_block_from_code(
        &self,
        block: PyTcRef<Block>,
        block_ctx: ParseContext,
    ) -> TurnipTextContextlessError;
    fn err_on_source(&self, src_ctx: ParseContext) -> TurnipTextContextlessError;
    fn on_inline(
        &mut self,
        py: Python,
        inl: &PyAny,
        inl_ctx: ParseContext,
    ) -> TurnipTextContextlessResult<()>;
}

/// When encountering inline content at the block level, build a Paragraph block
pub struct ParagraphInlineMode {
    ctx: ParseContext,
    para: Py<Paragraph>,
    start_of_line: bool,
    current_sentence: Py<Sentence>,
}
// Implement constructors for InlineLevelProcessor<Paragraph>
impl InlineLevelProcessor<ParagraphInlineMode> {
    pub fn new_with_inline(
        py: Python,
        inline: &PyAny,
        inline_ctx: ParseContext,
    ) -> TurnipTextContextlessResult<Self> {
        let current_sentence = py_internal_alloc(py, Sentence::new_empty(py))?;
        current_sentence
            .borrow_mut(py)
            .push_inline(inline)
            .err_as_internal(py)?;
        Ok(Self {
            inner: ParagraphInlineMode {
                ctx: ParseContext::new(inline_ctx.first_tok(), inline_ctx.last_tok()),
                para: py_internal_alloc(py, Paragraph::new_empty(py))?,
                start_of_line: false,
                current_sentence,
            },
            current_building_text: InlineTextState::new(),
        })
    }
    pub fn new_with_starting_text(
        py: Python,
        text: &str,
        text_span: ParseSpan,
    ) -> TurnipTextContextlessResult<Self> {
        Ok(Self {
            inner: ParagraphInlineMode {
                ctx: ParseContext::new(text_span, text_span),
                para: py_internal_alloc(py, Paragraph::new_empty(py))?,
                start_of_line: false,
                current_sentence: py_internal_alloc(py, Sentence::new_empty(py))?,
            },
            current_building_text: InlineTextState::new_with_text(text, text_span),
        })
    }
}
impl ParagraphInlineMode {
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
impl InlineMode for ParagraphInlineMode {
    fn inline_mode_ctx(&self) -> InlineModeContext {
        InlineModeContext::Paragraph(self.ctx)
    }

    fn ignore_whitespace(&self) -> bool {
        // Swallow whitespace at the start of the line
        self.start_of_line
    }

    fn on_newline(&mut self, py: Python, tok: TTToken) -> TurnipTextContextlessResult<ProcStatus> {
        // Always extend our token span so error messages using it have the full context
        assert!(
            self.ctx.try_extend(&tok.token_span()),
            "ParagraphInlineMode got a token from a different file that it was opened in"
        );
        // Text is already folded into the sentence
        if self.start_of_line {
            Ok(ProcStatus::PopAndReprocessToken(Some((
                self.ctx,
                BlockElem::Para(self.para.clone_ref(py)).into(),
            ))))
        } else {
            self.fold_current_sentence_into_paragraph(py)?;
            // We're now at the start of the line
            self.start_of_line = true;
            Ok(ProcStatus::Continue)
        }
    }

    fn on_eof(&mut self, py: Python, tok: TTToken) -> TurnipTextContextlessResult<ProcStatus> {
        assert!(
            self.ctx.try_extend(&tok.token_span()),
            "ParagraphInlineMode got a token from a different file that it was opened in"
        );
        if !self.start_of_line {
            self.fold_current_sentence_into_paragraph(py)?;
        }
        Ok(ProcStatus::PopAndReprocessToken(Some((
            self.ctx,
            BlockElem::Para(self.para.clone_ref(py)).into(),
        ))))
    }

    fn on_open_scope(
        &mut self,
        _py: Python,
        tok: TTToken,
        _data: &str,
    ) -> TurnipTextContextlessResult<ProcStatus> {
        Ok(ProcStatus::PushProcessor(rc_refcell(
            InlineLevelAmbiguousScopeProcessor::new(
                self.inline_mode_ctx(),
                self.start_of_line,
                tok.token_span(),
            ),
        )))
    }

    fn on_close_scope(
        &mut self,
        py: Python,
        tok: TTToken,
        _data: &str,
    ) -> TurnipTextContextlessResult<ProcStatus> {
        // If the closing brace is at the start of the line, it must be for block-scope and we can assume there won't be text afterwards.
        // End the paragraph, and tell the scope above us in the hierarchy to handle the scope close.
        if self.start_of_line {
            assert!(
                self.ctx.try_extend(&tok.token_span()),
                "ParagraphInlineMode got a token from a different file that it was opened in"
            );
            Ok(ProcStatus::PopAndReprocessToken(Some((
                self.ctx,
                BlockElem::Para(self.para.clone_ref(py)).into(),
            ))))
        } else {
            Err(InterpError::InlineScopeCloseOutsideScope(tok.token_span()).into())
        }
    }

    fn err_on_header_from_code(
        &self,
        header: PyTcRef<DocSegmentHeader>,
        header_ctx: ParseContext,
    ) -> TurnipTextContextlessError {
        // This must have come from code.
        if self.start_of_line {
            // Someone is trying to open a new, separate header and not a header "inside" the paragraph.
            // Give them a more relevant error message.
            InterpError::InsufficientBlockSeparation {
                last_block: BlockModeElem::Para(self.ctx),
                next_block_start: BlockModeElem::HeaderFromCode(header_ctx.full_span()),
            }
            .into()
        } else {
            // We're deep inside a paragraph here.
            InterpError::CodeEmittedHeaderInInlineMode {
                inl_mode: self.inline_mode_ctx(),
                header,
                code_span: header_ctx.full_span(),
            }
            .into()
        }
    }

    fn err_on_block_from_code(
        &self,
        block: PyTcRef<Block>,
        block_ctx: ParseContext,
    ) -> TurnipTextContextlessError {
        // This must have come from code.
        if self.start_of_line {
            // Someone is trying to open a new, separate block and not a block "inside" the paragraph.
            // Give them a more relevant error message.
            InterpError::InsufficientBlockSeparation {
                last_block: BlockModeElem::Para(self.ctx),
                next_block_start: BlockModeElem::BlockFromCode(block_ctx.full_span()),
            }
            .into()
        } else {
            // We're deep inside a paragraph here.
            InterpError::CodeEmittedBlockInInlineMode {
                inl_mode: self.inline_mode_ctx(),
                block,
                code_span: block_ctx.full_span(),
            }
            .into()
        }
    }

    fn err_on_source(&self, src_ctx: ParseContext) -> TurnipTextContextlessError {
        if self.start_of_line {
            // Someone is trying to open a new file separately from the paragraph and not "inside" the paragraph.
            // Give them a more relevant error message.
            InterpError::InsufficientBlockSeparation {
                last_block: BlockModeElem::Para(self.ctx),
                next_block_start: BlockModeElem::SourceFromCode(src_ctx.full_span()),
            }
            .into()
        } else {
            InterpError::CodeEmittedSourceInInlineMode {
                inl_mode: self.inline_mode_ctx(),
                code_span: src_ctx.full_span(),
            }
            .into()
        }
    }

    fn on_content(&mut self) {
        self.start_of_line = false;
    }

    fn on_inline(
        &mut self,
        py: Python,
        inl: &PyAny,
        inl_ctx: ParseContext,
    ) -> TurnipTextContextlessResult<()> {
        assert!(
            self.ctx.try_combine(inl_ctx),
            "ParagraphInlineMode got a token from a different file that it was opened in"
        );
        self.current_sentence
            .borrow_mut(py)
            .push_inline(inl)
            .err_as_internal(py)?;
        Ok(())
    }
}

/// Parser for a scope that is known to be an inline scope, i.e. has content on the same line as the scope open.
pub struct KnownInlineScopeInlineMode {
    preceding_inline: Option<InlineModeContext>,
    ctx: ParseContext,
    inline_scope: Py<InlineScope>,
    start_of_scope: bool,
}
// Implement constructor for InlineLevelProcessor<KnownInlineScope>
impl InlineLevelProcessor<KnownInlineScopeInlineMode> {
    pub fn new(
        py: Python,
        preceding_inline: Option<InlineModeContext>,
        ctx: ParseContext,
    ) -> TurnipTextContextlessResult<Self> {
        Ok(Self {
            inner: KnownInlineScopeInlineMode {
                preceding_inline,
                ctx,
                start_of_scope: true,
                inline_scope: py_internal_alloc(py, InlineScope::new_empty(py))?,
            },
            current_building_text: InlineTextState::new(),
        })
    }
}
impl InlineMode for KnownInlineScopeInlineMode {
    fn inline_mode_ctx(&self) -> InlineModeContext {
        InlineModeContext::InlineScope {
            scope_start: self.ctx.first_tok(),
        }
    }

    fn ignore_whitespace(&self) -> bool {
        // Swallow whitespace at the start of the scope
        self.start_of_scope
    }

    fn on_newline(&mut self, py: Python, tok: TTToken) -> TurnipTextContextlessResult<ProcStatus> {
        if self.inline_scope.borrow_mut(py).__len__(py) == 0 {
            unreachable!(
                "KnownInlineScopeInlineMode received a newline with no preceding content - was \
                 actually a block scope"
            );
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
    ) -> TurnipTextContextlessResult<ProcStatus> {
        let new_scopes_inline_context = match self.preceding_inline {
            // If we're part of a paragraph, the inner scope is part of a paragraph too
            // Just extend the paragraph to include the current token
            // TODO test this gets all the tokens up to but not including the inline scope
            Some(InlineModeContext::Paragraph(preceding_para)) => {
                let mut new = preceding_para.clone();
                assert!(
                    new.try_combine(self.ctx),
                    "Paragraph, KnownInlineScopeInlineMode don't generate source files, must \
                     always receive tokens from the same file"
                );
                InlineModeContext::Paragraph(new)
            }
            // If we aren't part of a paragraph, say the inner builder is in inline mode because of us
            None | Some(InlineModeContext::InlineScope { .. }) => InlineModeContext::InlineScope {
                scope_start: self.ctx.first_tok(),
            },
        };
        Ok(ProcStatus::PushProcessor(rc_refcell(
            InlineLevelAmbiguousScopeProcessor::new(
                new_scopes_inline_context,
                false,
                tok.token_span(),
            ),
        )))
    }
    fn on_close_scope(
        &mut self,
        py: Python,
        tok: TTToken,
        _data: &str,
    ) -> TurnipTextContextlessResult<ProcStatus> {
        // pending text has already been folded in
        assert!(
            self.ctx.try_extend(&tok.token_span()),
            "KnownInlineScopeInlineMode got a token from a different file that it was opened in"
        );
        Ok(ProcStatus::Pop(Some((
            self.ctx,
            InlineElem::InlineScope(self.inline_scope.clone_ref(py)).into(),
        ))))
    }

    fn on_eof(&mut self, _py: Python, tok: TTToken) -> TurnipTextContextlessResult<ProcStatus> {
        Err(InterpError::EndedInsideScope {
            scope_start: self.ctx.first_tok(),
            eof_span: tok.token_span(),
        }
        .into())
    }

    fn err_on_header_from_code(
        &self,
        header: PyTcRef<DocSegmentHeader>,
        header_ctx: ParseContext,
    ) -> TurnipTextContextlessError {
        InterpError::CodeEmittedHeaderInInlineMode {
            inl_mode: self.inline_mode_ctx(),
            header,
            code_span: header_ctx.full_span(),
        }
        .into()
    }
    fn err_on_block_from_code(
        &self,
        block: PyTcRef<Block>,
        block_ctx: ParseContext,
    ) -> TurnipTextContextlessError {
        InterpError::CodeEmittedBlockInInlineMode {
            inl_mode: self.inline_mode_ctx(),
            block,
            code_span: block_ctx.full_span(),
        }
        .into()
    }

    fn err_on_source(&self, src_ctx: ParseContext) -> TurnipTextContextlessError {
        InterpError::CodeEmittedSourceInInlineMode {
            inl_mode: self.inline_mode_ctx(),
            code_span: src_ctx.full_span(),
        }
        .into()
    }

    fn on_content(&mut self) {
        self.start_of_scope = false;
    }

    fn on_inline(
        &mut self,
        py: Python,
        inl: &PyAny,
        inl_ctx: ParseContext,
    ) -> TurnipTextContextlessResult<()> {
        assert!(
            self.ctx.try_combine(inl_ctx),
            "ParagraphInlineMode got a token from a different file that it was opened in"
        );
        self.inline_scope
            .borrow_mut(py)
            .push_inline(inl)
            .err_as_internal(py)?;
        Ok(())
    }
}

/// This struct implements text and whitespace merging for InlineLevelProcessor.
struct InlineTextState {
    text: String,
    /// pending_whitespace is appended to `text` before new text is added, but can be ignored in certain scenarios.
    ///
    /// e.g. "the" + Whitespace(" ") => ("the", " ") - when the next token is "apple", becomes "the" + " " + "apple"
    /// but for "the" + Whitespace(" ") + Newline, the pending_whitespace is dropped.
    pending_whitespace: Option<String>,
    last_text_token_span: Option<ParseSpan>,
}
impl InlineTextState {
    fn new() -> Self {
        Self {
            text: String::new(),
            pending_whitespace: None,
            last_text_token_span: None,
        }
    }

    fn new_with_text(text: &str, text_span: ParseSpan) -> Self {
        Self {
            text: text.to_string(),
            pending_whitespace: None,
            last_text_token_span: Some(text_span),
        }
    }

    fn encounter_text(&mut self, tok: TTToken, data: &str) {
        if let Some(w) = std::mem::take(&mut self.pending_whitespace) {
            self.text.push_str(&w)
        }
        self.text.push_str(&tok.stringify_escaped(data));
        self.last_text_token_span = Some(tok.token_span());
    }

    fn encounter_whitespace(&mut self, tok: TTToken, data: &str) {
        let whitespace_content = tok.stringify_escaped(data);
        match &mut self.pending_whitespace {
            Some(w) => w.push_str(whitespace_content),
            None => self.pending_whitespace = Some(whitespace_content.to_string()),
        };
        // Whitespace still counts here - e.g. space between scope-close and scope-open is counted and should be captured
        self.last_text_token_span = Some(tok.token_span());
    }

    /// Take the text component (optionally including the pending whitespace), and put it into a Text() inline object if non-empty.
    /// Returns the text object and the parsespan of the last consumed token - if there was whitespace pending then this will be the last token of the pending whitespace.
    /// Resets the pending_whitespace regardless of include_whitespace argument.
    fn flush(
        &mut self,
        py: Python,
        include_whitespace: bool,
    ) -> TurnipTextContextlessResult<Option<(Py<Text>, ParseSpan)>> {
        if let Some(w) = std::mem::take(&mut self.pending_whitespace) {
            if include_whitespace {
                self.text.push_str(&w)
            }
        }
        if !self.text.is_empty() {
            let old_text = std::mem::replace(&mut self.text, String::new());
            Ok(Some((
                py_internal_alloc(py, Text::new_rs(py, &old_text))?,
                std::mem::take(&mut self.last_text_token_span).expect(
                    "!text.is_empty() so must have encountered text so must have set text_token",
                ),
            )))
        } else {
            Ok(None)
        }
    }

    /// Take the text component (optionally including the pending whitespace), and put it into a Text() inline object and pass it into the InlineMode processor if not empty.
    /// Resets the pending_whitespace regardless of include_whitespace argument.
    fn flush_into<T: InlineMode>(
        &mut self,
        py: Python,
        include_whitespace: bool,
        inner: &mut T,
    ) -> TurnipTextContextlessResult<()> {
        match self.flush(py, include_whitespace)? {
            Some((py_text, last_token)) => {
                inner.on_content();
                inner.on_inline(
                    py,
                    py_text.as_ref(py),
                    ParseContext::new(last_token, last_token),
                )
            }
            None => Ok(()),
        }
    }
}

impl<T: InlineMode> TokenProcessor for InlineLevelProcessor<T> {
    fn process_token(
        &mut self,
        py: Python,
        _py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<ProcStatus> {
        match tok {
            // Escaped newline => "Continue sentence"
            TTToken::Escaped(_, Escapable::Newline) => Ok(ProcStatus::Continue),
            // Other escaped content, lone backslash, hyphens and dashes, and any other text are all treated as content
            TTToken::Escaped(_, _)
            | TTToken::Backslash(_)
            | TTToken::HyphenMinuses(..)
            | TTToken::EnDash(_)
            | TTToken::EmDash(_)
            | TTToken::OtherText(_) => {
                self.inner.on_content();
                self.current_building_text.encounter_text(tok, data);
                Ok(ProcStatus::Continue)
            }
            TTToken::Whitespace(_) => {
                if !self.inner.ignore_whitespace() {
                    self.current_building_text.encounter_whitespace(tok, data);
                }
                Ok(ProcStatus::Continue)
            }

            // Whitespace between content and a newline/EOF/scope close/comment is *trailing*
            // and thus ignored
            TTToken::Newline(_) => {
                self.current_building_text
                    .flush_into(py, false, &mut self.inner)?;
                self.inner.on_newline(py, tok)
            }
            TTToken::EOF(_) => {
                self.current_building_text
                    .flush_into(py, false, &mut self.inner)?;
                self.inner.on_eof(py, tok)
            }
            TTToken::ScopeClose(_) => {
                self.current_building_text
                    .flush_into(py, false, &mut self.inner)?;
                self.inner.on_close_scope(py, tok, data)
            }
            // TODO a commented-out line midway through a paragraph shouldn't split the paragraph
            TTToken::Hashes(_, _) => {
                self.current_building_text
                    .flush_into(py, false, &mut self.inner)?;
                Ok(ProcStatus::PushProcessor(rc_refcell(
                    CommentProcessor::new(),
                )))
            }

            // Whitespace between content and a scope open/code open/raw scope open is included
            TTToken::ScopeOpen(_) => {
                self.current_building_text
                    .flush_into(py, true, &mut self.inner)?;
                self.inner.on_open_scope(py, tok, data)
            }

            // Note this may return Block
            TTToken::CodeOpen(start_span, n_brackets) => {
                self.current_building_text
                    .flush_into(py, true, &mut self.inner)?;
                Ok(ProcStatus::PushProcessor(rc_refcell(CodeProcessor::new(
                    start_span, n_brackets,
                ))))
            }

            TTToken::RawScopeOpen(start_span, n_opening) => {
                self.current_building_text
                    .flush_into(py, true, &mut self.inner)?;
                Ok(ProcStatus::PushProcessor(rc_refcell(
                    RawStringProcessor::new(start_span, n_opening),
                )))
            }

            // Can't close a scope for a state we aren't in
            TTToken::CodeClose(span, _) => Err(InterpError::CodeCloseOutsideCode(span).into()),

            TTToken::RawScopeClose(span, _) => {
                Err(InterpError::RawScopeCloseOutsideRawScope(span).into())
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
                // Can't get a header or a block in an inline scope
                DocElement::HeaderFromCode(header) => {
                    Err(self.inner.err_on_header_from_code(header, elem_ctx))
                }
                DocElement::Block(BlockElem::FromCode(block)) => {
                    Err(self.inner.err_on_block_from_code(block, elem_ctx))
                }
                DocElement::Block(BlockElem::BlockScope(_)) => {
                    unreachable!("InlineLevelProcessor never tries to build a BlockScope")
                }
                DocElement::Block(BlockElem::Para(_)) => {
                    unreachable!("InlineLevelProcessor never tries to build an inner Paragraph")
                }
                // If we get an inline, shove it in
                DocElement::Inline(inline) => {
                    self.inner.on_content();
                    self.inner.on_inline(py, inline.as_ref(py), elem_ctx)?;
                    Ok(ProcStatus::Continue)
                }
            },
            None => {
                self.inner.on_content();
                Ok(ProcStatus::Continue)
            }
        }
    }

    fn on_emitted_source_inside(
        &mut self,
        code_emitting_source: ParseContext,
    ) -> TurnipTextContextlessResult<()> {
        Err(self.inner.err_on_source(code_emitting_source))
    }

    fn on_emitted_source_closed(&mut self, _inner_source_emitted_by: ParseSpan) {
        unreachable!("InlineLevelProcessor always returns Err on_emitted_source_inside")
    }
}

/// Processor that generates a raw string, ending on raw-scope-closes with the correct number of hash characters.
pub struct RawStringProcessor {
    ctx: ParseContext,
    n_closing: usize,
    raw_data: String,
}
impl RawStringProcessor {
    pub fn new(start_span: ParseSpan, n_opening: usize) -> Self {
        Self {
            ctx: ParseContext::new(start_span, start_span),
            n_closing: n_opening,
            raw_data: String::new(),
        }
    }
}
impl TokenProcessor for RawStringProcessor {
    fn process_token(
        &mut self,
        py: Python,
        _py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<ProcStatus> {
        // This builder does not directly emit new source files, so it cannot receive tokens from inner files.
        // When receiving EOF it returns an error.
        // This fulfils the contract for [TokenProcessor::process_token].
        match tok {
            TTToken::RawScopeClose(_, given_closing) if given_closing == self.n_closing => {
                self.ctx.try_extend(&tok.token_span());
                let raw = py_internal_alloc(
                    py,
                    Raw::new_rs(py, std::mem::take(&mut self.raw_data).as_str()),
                )?;
                Ok(ProcStatus::Pop(Some((
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
                Ok(ProcStatus::Continue)
            }
        }
    }

    fn process_emitted_element(
        &mut self,
        _py: Python,
        _py_env: &PyDict,
        _pushed: Option<EmittedElement>,
        // closing_token: TTToken,
    ) -> TurnipTextContextlessResult<ProcStatus> {
        panic!("RawStringProcessor does not spawn inner builders")
    }

    fn on_emitted_source_inside(
        &mut self,
        _code_emitting_source: ParseContext,
    ) -> TurnipTextContextlessResult<()> {
        unreachable!(
            "RawStringProcessor does not spawn an inner code builder, so cannot have a source \
             file emitted inside"
        )
    }
    fn on_emitted_source_closed(&mut self, _inner_source_emitted_by: ParseSpan) {
        unreachable!(
            "RawStringProcessor does not spawn an inner code builder, so cannot have a source \
             file emitted inside"
        )
    }
}
