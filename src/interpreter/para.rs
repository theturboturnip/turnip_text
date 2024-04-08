/*
use pyo3::{prelude::*, types::PyDict};

use crate::{
    error::TurnipTextContextlessError,
    interpreter::{eval_brackets, EvalBracketResult, InterpError},
    lexer::{Escapable, TTToken},
    util::ParseSpan,
};

use super::{
    InlineNodeToCreate, InterpBlockTransition, InterpSpecialTransition, MapContextlessResult,
    TurnipTextContextlessResult,
};

use super::python::{
    interop::*,
    typeclass::{PyTcRef, PyTcUnionRef},
};

#[derive(Debug)]
pub(crate) struct InterpParaState {
    para: Py<Paragraph>,
    sentence: Py<Sentence>,
    inline_stack: Vec<InterpInlineScopeState>,
    sentence_state: InterpSentenceState,
}

#[derive(Debug)]
struct InterpInlineScopeState {
    builder: Option<PyTcRef<InlineScopeBuilder>>,
    children: Py<InlineScope>,
    scope_start: ParseSpan,
}
impl InterpInlineScopeState {
    fn build_to_inline(
        self,
        py: Python,
        scope_end: ParseSpan,
    ) -> TurnipTextContextlessResult<PyTcRef<Inline>> {
        let scope = self.scope_start.combine(&scope_end);
        match self.builder {
            Some(builder) => InlineScopeBuilder::call_build_from_inlines(py, builder, self.children)
                .err_as_interp(py, "Error while calling .build_from_inlines() on an object", scope),
            None => Ok(PyTcRef::of(self.children.as_ref(py)).expect("Internal error: InterpInlineScopeState::children, an InlineScope, somehow doesn't fit the Inline typeclass")),
        }
    }
}

/// Interpreter state specific to parsing paragraphs and the content within (i.e. inline content)
#[derive(Debug)]
enum InterpSentenceState {
    /// When at the start of a sentence, ready for any inline token
    SentenceStart,
    /// When in the middle of a sentence, ready for any inline token
    MidSentence,
    /// After encountering text, allow further text to be merged in
    BuildingText {
        text: String,
        /// pending_whitespace is appended to `text` before new text is added, but can be ignored in certain scenarios.
        ///
        /// e.g. "the" + Whitespace(" ") => ("the", " ") - when the next token is "apple", becomes "the" + " " + "apple"
        /// but for "the" + Whitespace(" ") + Newline, the pending_whitespace is dropped.
        pending_whitespace: Option<String>,
    },
    /// When in code mode
    BuildingCode {
        code: String,
        code_start: ParseSpan,
        expected_n_hashes: usize,
    },
    /// When building raw text, optionally attached to a RawScopeBuilder
    BuildingRawText {
        builder: Option<PyTcRef<RawScopeBuilder>>,
        text: String,
        /// Either the token opening the raw scope, or the code for the builder
        raw_start: ParseSpan,
        expected_n_hashes: usize,
    },
}

#[derive(Debug)]
pub(crate) enum InterpParaTransition {
    /// On encountering inline content within a paragraph, add it to the paragraph (starting a new one if necessary).
    ///
    /// - [InterpSentenceState::SentenceStart] -> [InterpSentenceState::MidSentence]
    /// - [InterpSentenceState::MidSentence] -> [InterpSentenceState::MidSentence]
    /// - [InterpSentenceState::BuildingCode] -> [InterpSentenceState::MidSentence]
    /// - [InterpSentenceState::BuildingRawText] -> [InterpSentenceState::MidSentence]
    PushInlineContent(InlineNodeToCreate),

    /// Break the current sentence within the paragraph.
    /// Finishes the current BuildingText if in progress, and pushes it to the topmost scope (which should be the sentence)
    /// Errors out if inline scopes are currently open - right now inline scopes must be entirely within a sentence.
    ///
    /// - [InterpSentenceState::MidSentence] -> [InterpSentenceState::SentenceStart]
    BreakSentence,

    /// On running Python code and returning None, ignore the code without emitting anything
    ///
    /// - [InterpSentenceState::BuildingCode] -> [InterpSentenceState::MidSentence]
    EmitNone,

    /// On encountering the start of an inline scope (i.e. an InlineScopeOpen optionally preceded by Python scope owner),
    /// push an inline scope onto existing paragraph state (or create a new one).
    ///
    /// Finishes the current BuildingText if in progress, pushes it to topmost scope before creating new scope.
    ///
    /// - [InterpSentenceState::SentenceStart] -> [InterpSentenceState::MidSentence]
    /// - [InterpSentenceState::MidSentence] -> [InterpSentenceState::MidSentence]
    /// - [InterpSentenceState::BuildingCode] -> [InterpSentenceState::MidSentence]
    /// - [InterpSentenceState::BuildingText] -> [InterpSentenceState::MidSentence]
    PushInlineScope(Option<PyTcRef<InlineScopeBuilder>>, ParseSpan),

    /// On encountering a scope close, pop the current inline scope off the stack
    /// (pushing the current BuildingText to that scope beforehand)
    /// (throwing an error if the stack is empty)
    /// - [InterpSentenceState::SentenceStart] -> [InterpSentenceState::MidSentence]
    /// - [InterpSentenceState::MidSentence] -> [InterpSentenceState::MidSentence]
    /// - [InterpSentenceState::BuildingText] -> [InterpSentenceState::MidSentence]
    PopInlineScope(ParseSpan),

    /// On encountering code within a paragraph, end the current inline token and enter code mode.
    /// (pushing the current BuildingText to that scope beforehand)
    ///
    /// - [InterpSentenceState::SentenceStart] -> [InterpSentenceState::BuildingCode]
    /// - [InterpSentenceState::MidSentence] -> [InterpSentenceState::BuildingCode]
    /// - [InterpSentenceState::BuildingText] -> [InterpSentenceState::BuildingCode]
    StartInlineLevelCode(ParseSpan, usize),

    /// See [InterpBlockTransition::EndParagraph]
    ///
    /// Finish the paragraph and current sentence (raising an error if processing inline scopes)
    ///
    /// Contains None if request was brought up by EOF
    ///
    /// - [InterpSentenceState::SentenceStart] -> (other block state)
    EndParagraph(Option<ParseSpan>),

    /// See [InterpBlockTransition::EndParagraphAndPopBlock]
    ///
    /// Finish the paragraph and current sentence (raising an error if processing inline scopes),
    /// and pop the block
    ///
    /// - [InterpSentenceState::SentenceStart] -> (other block state)
    EndParagraphAndPopBlock(ParseSpan),

    /// Finishes the current BuildingText if in progress, pushes it to topmost scope, enters comment mode
    ///
    /// Breaks the current sentence
    ///
    /// - [InterpSentenceState::SentenceStart] -> (comment mode) + [InterpSentenceState::MidSentence]
    /// - [InterpSentenceState::MidSentence] -> (comment mode) + [InterpSentenceState::MidSentence]
    /// - [InterpSentenceState::BuildingText] -> (comment mode) + [InterpSentenceState::MidSentence]
    StartComment(ParseSpan),

    /// On encountering a raw scope open, start processing a raw block of text.
    /// Finishes the current BuildingText if in progress, pushes it to topmost scope.
    ///
    /// - [InterpSentenceState::SentenceStart] -> [InterpSentenceState::BuildingRawText]
    /// - [InterpSentenceState::MidSentence] -> [InterpSentenceState::BuildingRawText]
    /// - [InterpSentenceState::BuildingText] -> [InterpSentenceState::BuildingRawText]
    /// - (other block state) -> [InterpSentenceState::BuildingRawText]
    StartRawScope(Option<PyTcRef<RawScopeBuilder>>, ParseSpan, usize),

    /// On encountering inline text, start processing a string of text
    ///
    /// - [InterpSentenceState::SentenceStart] -> [InterpSentenceState::BuildingText]
    /// - [InterpSentenceState::MidSentence] -> [InterpSentenceState::BuildingText]
    /// - (other block state) -> [InterpSentenceState::BuildingText]
    StartText(String),
}

impl InterpParaState {
    pub(crate) fn new(py: Python) -> PyResult<Self> {
        Ok(Self {
            sentence_state: InterpSentenceState::SentenceStart,
            para: Py::new(py, Paragraph::new_empty(py))?,
            sentence: Py::new(py, Sentence::new_empty(py))?,
            inline_stack: vec![],
        })
    }

    pub(crate) fn para(&self) -> &Py<Paragraph> {
        &self.para
    }

    pub(crate) fn finalize(
        &mut self,
        py: Python,
    ) -> TurnipTextContextlessResult<(
        Option<InterpBlockTransition>,
        Option<InterpSpecialTransition>,
    )> {
        match self.sentence_state {
            InterpSentenceState::SentenceStart | InterpSentenceState::MidSentence => {
                // This will automatically check if we're inside an inline scope
                self.handle_transition(py, Some(InterpParaTransition::EndParagraph(None)))
            }
            InterpSentenceState::BuildingText { .. } => {
                self.handle_transition(py, Some(InterpParaTransition::BreakSentence))?;
                // This will automatically check if we're inside an inline scope
                self.handle_transition(py, Some(InterpParaTransition::EndParagraph(None)))
            }
            // Error states
            InterpSentenceState::BuildingCode { code_start, .. } => {
                return Err(InterpError::EndedInsideCode { code_start }.into())
            }
            InterpSentenceState::BuildingRawText { raw_start, .. } => {
                return Err(InterpError::EndedInsideRawScope {
                    raw_scope_start: raw_start,
                }
                .into())
            }
        }
    }

    pub(crate) fn handle_token(
        &mut self,
        py: Python,
        py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<(
        Option<InterpBlockTransition>,
        Option<InterpSpecialTransition>,
    )> {
        let transition = self.mutate_state_and_find_transition(py, py_env, tok, data)?;
        self.handle_transition(py, transition)
    }
    pub(crate) fn handle_transition(
        &mut self,
        py: Python,
        transition: Option<InterpParaTransition>,
    ) -> TurnipTextContextlessResult<(
        Option<InterpBlockTransition>,
        Option<InterpSpecialTransition>,
    )> {
        if let Some(transition) = transition {
            use InterpParaTransition as T;
            use InterpSentenceState as S;

            // All transitions interrupt the current Text token
            if let S::BuildingText {
                text,
                pending_whitespace,
            } = &mut self.sentence_state
            {
                if let Some(pw) = pending_whitespace {
                    // Decide if we want to push the pending_whitespace
                    let combine_whitespace = match &transition {
                        T::BreakSentence
                        | T::EndParagraph(_)
                        | T::EndParagraphAndPopBlock(_)
                        | T::StartComment(_) => false,
                        _ => true,
                    };
                    if combine_whitespace {
                        text.push_str(pw);
                        *pending_whitespace = None;
                    }
                }
                // Finish the text-in-progress and push to topmost scope
                let node = InlineNodeToCreate::Text(text.clone()).to_py(py)?;
                self.push_to_topmost_scope(py, node.as_ref(py))?;
            }

            let (new_inl_state, transitions) = match (&self.sentence_state, transition) {
                (S::SentenceStart | S::MidSentence, T::StartText(text)) => (
                    S::BuildingText {
                        text,
                        pending_whitespace: None,
                    },
                    (None, None),
                ),

                (
                    S::SentenceStart
                    | S::MidSentence
                    | S::BuildingCode { .. }
                    | S::BuildingRawText { .. }
                    | S::BuildingText { .. },
                    T::PushInlineContent(content),
                ) => {
                    let content = content.to_py(py)?;
                    self.push_to_topmost_scope(py, content.as_ref(py))?;
                    (S::MidSentence, (None, None))
                }
                (S::MidSentence | S::BuildingText { .. }, T::BreakSentence) => {
                    // Ensure we don't have any inline scopes
                    self.check_inline_scopes_closed().map_err(|scope_start| {
                        InterpError::SentenceBreakInInlineScope { scope_start }
                    })?;
                    self.break_sentence(py)?;
                    (S::SentenceStart, (None, None))
                }
                (
                    S::SentenceStart | S::MidSentence | S::BuildingText { .. },
                    T::StartComment(span),
                ) => {
                    // Ensure we don't have any inline scopes
                    self.check_inline_scopes_closed().map_err(|scope_start| {
                        InterpError::SentenceBreakInInlineScope { scope_start }
                    })?;
                    self.break_sentence(py)?;
                    (
                        S::SentenceStart,
                        (None, Some(InterpSpecialTransition::StartComment(span))),
                    )
                }

                (S::BuildingCode { .. }, T::EmitNone) => (S::MidSentence, (None, None)),

                (
                    S::SentenceStart
                    | S::MidSentence
                    | S::BuildingCode { .. }
                    | S::BuildingText { .. },
                    T::PushInlineScope(builder, span),
                ) => {
                    let scope = InterpInlineScopeState {
                        builder,
                        children: Py::new(py, InlineScope::new_empty(py)).err_as_internal(py)?,
                        scope_start: span,
                    };
                    self.inline_stack.push(scope);
                    (S::MidSentence, (None, None))
                }
                (
                    S::SentenceStart | S::MidSentence | S::BuildingText { .. },
                    T::PopInlineScope(scope_end),
                ) => {
                    let popped_scope = self.inline_stack.pop();
                    match popped_scope {
                        Some(popped_scope) => {
                            let inline_item = popped_scope.build_to_inline(py, scope_end)?;
                            self.push_to_topmost_scope(py, inline_item.as_ref(py))?
                        },
                        None => {
                            return Err(TurnipTextContextlessError::Internal("PopInlineScope attempted with no inline scopes - should use EndParagraphAndPopBlock in this case".into()))
                        }
                    };
                    (S::MidSentence, (None, None))
                }

                (
                    S::SentenceStart
                    | S::MidSentence
                    | S::BuildingText { .. }
                    | S::BuildingCode { .. },
                    T::StartRawScope(builder, raw_start, expected_n_hashes),
                ) => (
                    S::BuildingRawText {
                        builder,
                        text: "".into(),
                        raw_start,
                        expected_n_hashes,
                    },
                    (None, None),
                ),

                (
                    S::SentenceStart | S::MidSentence | S::BuildingText { .. },
                    T::StartInlineLevelCode(code_start, expected_n_hashes),
                ) => (
                    S::BuildingCode {
                        code: "".into(),
                        code_start,
                        expected_n_hashes,
                    },
                    (None, None),
                ),

                (
                    S::SentenceStart | S::MidSentence | S::BuildingText { .. },
                    T::EndParagraph(para_break),
                ) => {
                    if let Err(scope_start) = self.check_inline_scopes_closed() {
                        if let Some(para_break) = para_break {
                            return Err(InterpError::ParaBreakInInlineScope {
                                scope_start,
                                para_break,
                            }
                            .into());
                        } else {
                            return Err(InterpError::EndedInsideScope { scope_start }.into());
                        }
                    }
                    self.break_sentence(py)?;
                    (
                        S::SentenceStart,
                        (Some(InterpBlockTransition::EndParagraph), None),
                    )
                }

                (
                    S::SentenceStart | S::MidSentence | S::BuildingText { .. },
                    T::EndParagraphAndPopBlock(scope_end_span),
                ) => {
                    // This is only called when all inline scopes are closed - just assert they are
                    self.check_inline_scopes_closed().map_err(|_| {
                        TurnipTextContextlessError::Internal("paragraph EndParagraphAndPopBlock transition invoked when inline scopes are still on the stack".into())
                    })?;
                    self.break_sentence(py)?;
                    (
                        S::SentenceStart,
                        (
                            Some(InterpBlockTransition::EndParagraphAndCloseManualBlockScope(
                                scope_end_span,
                            )),
                            None,
                        ),
                    )
                }

                (_, transition) => {
                    return Err(TurnipTextContextlessError::Internal(
                        format!(
                            "Invalid inline state/transition pair encountered ({0:?}, {1:?})",
                            self.sentence_state, transition
                        )
                        .into(),
                    ))
                }
            };
            self.sentence_state = new_inl_state;
            Ok(transitions)
        } else {
            Ok((None, None))
        }
    }
    /// Helper function
    fn break_sentence(&mut self, py: Python) -> TurnipTextContextlessResult<()> {
        // If the sentence has stuff in it, push it into the paragraph and make a new one
        if self.sentence.borrow(py).__len__(py) > 0 {
            self.para
                .borrow_mut(py)
                .push_sentence(self.sentence.as_ref(py))
                .err_as_internal(py)?;
            self.sentence = Py::new(py, Sentence::new_empty(py)).err_as_internal(py)?;
        }
        Ok(())
    }

    fn mutate_state_and_find_transition(
        &mut self,
        py: Python,
        py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<Option<InterpParaTransition>> {
        use InterpParaTransition::*;
        use TTToken::*;

        let transition = match &mut self.sentence_state {
            InterpSentenceState::SentenceStart => match tok {
                // Escaped newline => "Continue sentence".
                // at the start of a sentence, "Continue sentence" has no meaning
                Escaped(_, Escapable::Newline) => None,
                // Ignore whitespace at the start of lines
                Whitespace(_) => None,

                Newline(span) => Some(EndParagraph(Some(span))),
                Hashes(span, _) => Some(StartComment(span)),

                CodeOpen(span, n) => Some(StartInlineLevelCode(span, n)),
                BlockScopeOpen(span) => {
                    return Err(InterpError::BlockScopeOpenedMidPara { scope_start: span }.into())
                }
                InlineScopeOpen(span) => Some(PushInlineScope(None, span)),
                RawScopeOpen(span, n) => Some(StartRawScope(None, span, n)),

                CodeClose(span, _) => return Err(InterpError::CodeCloseOutsideCode(span).into()),
                ScopeClose(span) => Some(self.try_pop_scope(py, span)?),
                RawScopeClose(span, _) => {
                    return Err(InterpError::RawScopeCloseOutsideRawScope(span).into())
                }

                _ => Some(StartText(tok.stringify_escaped(data).into())),
            },
            InterpSentenceState::MidSentence => match tok {
                // Escaped newline => "Continue sentence" i.e. no sentence break
                // mid-sentence, "Continue sentence" just means "do nothing"
                Escaped(_, Escapable::Newline) => None,

                // Newline => Sentence break
                Newline(_) => Some(BreakSentence),
                Hashes(span, _) => Some(StartComment(span)),

                CodeOpen(span, n) => Some(StartInlineLevelCode(span, n)),
                BlockScopeOpen(span) => {
                    return Err(InterpError::BlockScopeOpenedMidPara { scope_start: span }.into())
                }
                InlineScopeOpen(span) => Some(PushInlineScope(None, span)),
                RawScopeOpen(span, n) => Some(StartRawScope(None, span, n)),

                CodeClose(span, _) => return Err(InterpError::CodeCloseOutsideCode(span).into()),
                ScopeClose(span) => Some(self.try_pop_scope(py, span)?),
                RawScopeClose(span, _) => {
                    return Err(InterpError::RawScopeCloseOutsideRawScope(span).into())
                }

                // Whitespace is included in text
                Whitespace(_) | _ => Some(StartText(tok.stringify_escaped(data).into())),
            },
            InterpSentenceState::BuildingText {
                text,
                pending_whitespace,
            } => match tok {
                // Escaped newline => "Continue sentence".
                // mid-sentence, "Continue sentence" has no meaning
                Escaped(_, Escapable::Newline) => None,

                // Newline => Sentence break
                Newline(_) => Some(BreakSentence),
                Hashes(span, _) => Some(StartComment(span)),

                CodeOpen(span, n) => Some(StartInlineLevelCode(span, n)),
                BlockScopeOpen(span) => {
                    return Err(InterpError::BlockScopeOpenedMidPara { scope_start: span }.into())
                }
                InlineScopeOpen(span) => Some(PushInlineScope(None, span)),
                RawScopeOpen(span, n) => Some(StartRawScope(None, span, n)),

                CodeClose(span, _) => return Err(InterpError::CodeCloseOutsideCode(span).into()),
                ScopeClose(span) => Some(self.try_pop_scope(py, span)?),
                RawScopeClose(span, _) => {
                    return Err(InterpError::RawScopeCloseOutsideRawScope(span).into())
                }

                // Whitespace is pushed to pending_whitespace, and no transition takes place.
                Whitespace(_) => {
                    match pending_whitespace {
                        Some(pw) => pw.push_str(tok.stringify_escaped(data)),
                        None => *pending_whitespace = Some(tok.stringify_escaped(data).into()),
                    };
                    None
                }
                // Pushing normal text pushes (and zeroes) pending_whitespace if present, then pushes the text itself
                _ => {
                    if let Some(pw) = pending_whitespace {
                        text.push_str(pw);
                        *pending_whitespace = None;
                    }
                    text.push_str(tok.stringify_escaped(data));
                    None
                }
            },
            InterpSentenceState::BuildingCode {
                code,
                code_start,
                expected_n_hashes,
            } => {
                match eval_brackets(data, tok, code, code_start, *expected_n_hashes, py, py_env)? {
                    Some((res, code_span)) => {
                        // The code ended...
                        use EvalBracketResult::*;

                        let inl_transition = match res {
                            NeededBlockBuilder(_) => {
                                return Err(InterpError::BlockOwnerCodeMidPara { code_span }.into())
                            }
                            // This is inline code because we're already deep into a paragraph at this point.
                            // Definitely can't emit a segment here.
                            DocSegmentHeader(_) => {
                                return Err(
                                    InterpError::DocSegmentHeaderMidPara { code_span }.into()
                                )
                            }
                            Block(_) => {
                                return Err(InterpError::BlockCodeMidPara { code_span }.into())
                            }
                            TurnipTextSource(_) => {
                                return Err(InterpError::InsertedFileMidPara { code_span }.into())
                            }
                            NeededInlineBuilder(i) => PushInlineScope(Some(i), code_span),
                            NeededRawBuilder(r, n_hashes) => {
                                StartRawScope(Some(r), code_span, n_hashes)
                            }
                            // This includes coerced objects - e.g. eval bracket returning a string gets wrapped in Text automatically
                            Inline(i) => PushInlineContent(InlineNodeToCreate::PythonObject(i)),
                            PyNone => EmitNone,
                        };
                        Some(inl_transition)
                    }
                    None => None,
                }
            }
            InterpSentenceState::BuildingRawText {
                builder,
                text,
                expected_n_hashes,
                raw_start,
            } => match tok {
                RawScopeClose(_, n_hashes) if n_hashes == *expected_n_hashes => match builder {
                    Some(builder) => {
                        let to_emit = RawScopeBuilder::call_build_from_raw(py, builder, text)
                            .err_as_interp(
                                py,
                                "Error while calling .build_from_raw() on an object",
                                *raw_start,
                            )?;

                        match to_emit {
                            PyTcUnionRef::A(inl) => {
                                Some(PushInlineContent(InlineNodeToCreate::PythonObject(inl)))
                            }
                            PyTcUnionRef::B(_) => {
                                return Err(InterpError::BlockCodeFromRawScopeMidPara {
                                    code_span: *raw_start,
                                }
                                .into())
                            }
                        }
                    }
                    None => Some(PushInlineContent(InlineNodeToCreate::Raw(text.clone()))),
                },

                _ => {
                    text.push_str(tok.stringify_raw(data));
                    None
                }
            },
        };
        Ok(transition)
    }

    fn try_pop_scope(
        &mut self,
        py: Python,
        scope_close_span: ParseSpan,
    ) -> TurnipTextContextlessResult<InterpParaTransition> {
        match self.inline_stack.last() {
            Some(InterpInlineScopeState { .. }) => {
                Ok(InterpParaTransition::PopInlineScope(scope_close_span))
            }
            None => {
                // If the sentence has stuff in it, push it into the paragraph and make a new one
                if self.sentence.borrow(py).__len__(py) > 0 {
                    self.para
                        .borrow_mut(py)
                        .push_sentence(self.sentence.as_ref(py))
                        .err_as_internal(py)?;
                    self.sentence = Py::new(py, Sentence::new_empty(py)).err_as_internal(py)?;
                }
                Ok(InterpParaTransition::EndParagraphAndPopBlock(
                    scope_close_span,
                ))
            }
        }
    }

    /// Check if all inline scopes are closed, returning [Err] of [ParseSpan] of the closest open inline scope if not.
    fn check_inline_scopes_closed(&self) -> Result<(), ParseSpan> {
        if let Some(i) = self.inline_stack.last() {
            Err(i.scope_start)
        } else {
            Ok(())
        }
    }

    fn push_to_topmost_scope(&self, py: Python, node: &PyAny) -> TurnipTextContextlessResult<()> {
        match self.inline_stack.last() {
            Some(i) => i.children.borrow_mut(py).push_inline(node),
            None => self.sentence.borrow_mut(py).push_inline(node),
        }
        .err_as_internal(py)
    }
}

*/
