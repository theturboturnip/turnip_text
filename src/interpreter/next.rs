//! The combinatorial explosion of moving between block/inline states hsa gotten too much to handle.
//! It's also inconvenient - e.g. `[thing]{contents}` may only emit an Inline, even if it's the only thing on the line and looks like it could emit Block, because the parser moves to "inline mode" and can't handle getting a Block out of that.
//! The correct course of action would be for the code builder to compute the inner inline, pass it into the builder, realize it got a Block out, and emit that block directly.
//! I'm envisioning a system with a stack of builders: every time a new token is received it's given to the topmost builder on the stack, which can return
//! - an error, stopping everything
//! - a completed object, implicitly popping the stack of builders
//! - a new builder to push to the stack
//!
//! If a completed object is returned, the builder is popped and the object is passed into the next builder on the stack to be integrated into the contents.
//! This method is convenient because it handles other alt-states for parsing such as comments and raw strings naturally by putting them on top of the stack!
//!
//! TODO this method currently doesn't split up BlockScopes by file - i.e. if you open a scope in one file, go into a subfile, then close it inside the subfile, that's allowed. Need to add a check inside BlockScopeBuilder(?) that the closing scope is in the same file as the opening scope, and need to block opening subfiles inside specific builders

use pyo3::{types::PyDict, Py, PyAny, PyClass, PyClassInitializer, PyResult, Python};

use crate::{
    error::{TurnipTextContextlessError, TurnipTextContextlessResult},
    lexer::{Escapable, LexError, TTToken},
    util::ParseSpan,
};

use super::{
    eval_bracket::{eval_brackets, EvalBracketResult},
    python::typeclass::PyTcRef,
    Block, BlockScope, DocSegment, DocSegmentHeader, Inline, InlineScope, InterimDocumentStructure,
    InterpError, InterpreterFileAction, MapContextlessResult, Paragraph, Raw, Sentence, Text,
    TurnipTextSource,
};

/// An enum encompassing all the things that can be directly emitted from one Builder to be bubbled up to the previous Builder.
///
/// Doesn't include TurnipTextSource - that is emitted from Python but it needs to bypass everything and go to the top-level interpreter
enum DocElement {
    Block(PyTcRef<Block>),
    Inline(PyTcRef<Inline>),
    Header(PyTcRef<DocSegmentHeader>),
    Raw(String),
}

struct PushToNextLevel {
    from_builder: BuilderContext,
    elem: DocElement,
}

enum BuildStatus {
    Done(Option<PushToNextLevel>),
    Continue,
    StartInnerBuilder(Box<dyn BuildFromTokens>),
    NewSource(TurnipTextSource),
}

#[derive(Debug, Clone, Copy)]
struct BuilderContext {
    builder_name: &'static str,
    from_span: ParseSpan,
}
impl BuilderContext {
    fn new(builder_name: &'static str, start_span: ParseSpan) -> Self {
        Self {
            builder_name,
            from_span: start_span,
        }
    }
    fn try_extend(&mut self, tok: &TTToken) -> bool {
        let span = tok.token_span();
        if span.file_idx() == self.from_span.file_idx() {
            self.from_span = self.from_span.combine(&span);
            true
        } else {
            false
        }
    }
    fn make(self, elem: DocElement) -> PushToNextLevel {
        PushToNextLevel {
            from_builder: self,
            elem,
        }
    }
}

trait BuildFromTokens {
    fn process_token(
        &mut self,
        py: Python,
        py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus>;
    fn process_push_from_inner_builder(
        &mut self,
        py: Python,
        py_env: &PyDict,
        pushed: Option<PushToNextLevel>,
        closing_tok: TTToken,
    ) -> TurnipTextContextlessResult<BuildStatus>;
    // TODO some boolean property for "let someone open an inserted file in the middle of this"
}

pub struct Interpreter {
    builders: BuilderStacks,
}
impl Interpreter {
    pub fn new(py: Python) -> PyResult<Self> {
        Ok(Self {
            builders: BuilderStacks::new(py)?,
        })
    }

    pub fn handle_tokens<'a>(
        &'a mut self,
        py: Python,
        py_env: &PyDict,
        toks: &mut impl Iterator<Item = Result<TTToken, LexError>>,
        file_idx: usize, // Attached to any LexError given
        data: &str,
    ) -> TurnipTextContextlessResult<InterpreterFileAction> {
        for tok in toks {
            let tok = tok.map_err(|lex_err| (file_idx, lex_err))?;
            match self.process_token(py, py_env, tok, data)? {
                None => continue,
                Some(TurnipTextSource { name, contents }) => {
                    return Ok(InterpreterFileAction::FileInserted { name, contents })
                }
            }
        }
        Ok(InterpreterFileAction::FileEnded)
    }

    fn process_token(
        &mut self,
        py: Python,
        py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<Option<TurnipTextSource>> {
        // Shove the token into the topmost builder and see what comes out
        let status = self
            .builders
            .top_builder()
            .process_token(py, py_env, tok, data)?;
        self.handle_build_status(py, py_env, tok, status)
    }

    fn handle_build_status(
        &mut self,
        py: Python,
        py_env: &PyDict,
        tok: TTToken,
        status: BuildStatus,
    ) -> TurnipTextContextlessResult<Option<TurnipTextSource>> {
        match status {
            BuildStatus::Done(pushed) => {
                self.builders.pop_from_current_file().expect("We just parsed something using the top of self.builders, it must be able to pop");
                let next_status = self
                    .builders
                    .top_builder()
                    .process_push_from_inner_builder(py, py_env, pushed, tok)?;
                return self.handle_build_status(py, py_env, tok, next_status);
            }
            BuildStatus::StartInnerBuilder(new_builder) => {
                self.builders.push_to_current_file(new_builder)
            }
            BuildStatus::NewSource(src) => return Ok(Some(src)),
            BuildStatus::Continue => {}
        };
        Ok(None)
    }

    pub fn push_subfile(&mut self) {
        self.builders.push_subfile()
    }

    pub fn pop_subfile(
        &mut self,
        py: Python,
        py_env: &'_ PyDict,
        data: &str,
    ) -> TurnipTextContextlessResult<()> {
        let mut stack = self.builders.pop_subfile();
        // The EOF token from the end of the file should have bubbled everything out.
        assert!(stack.is_empty());
        // // If there are any builders within the stack, tell them about the EOF and bubble their production up to the next level.
        // match stack.pop() {
        //     Some(mut builder) => {
        //         let mut pushed = builder.process_token(py, py_env)?;
        //         while let Some(mut builder) = stack.pop() {
        //             pushed = {
        //                 builder.process_push_from_inner_builder(py, py_env, pushed)?;
        //                 builder.process_eof(py, py_env)?
        //             };
        //         }
        //         // If there were builders, then a new element (which may be None!) was produced and we need to bubble it up to the next file
        //         self.builders
        //             .top_builder()
        //             .process_push_from_inner_builder(py, py_env, pushed)?;
        //     }
        //     // If there weren't any builders, we don't need to do anything
        //     None => {}
        // };
        Ok(())
    }

    pub fn finalize<'a>(
        mut self,
        py: Python,
        py_env: &PyDict,
    ) -> TurnipTextContextlessResult<Py<DocSegment>> {
        let (mut top, mut stack) = self.builders.finalize();

        // The EOF token from the end of the toplevel file should have bubbled everything out.
        assert!(stack.is_empty());

        // If there are any builders within the stack, tell them about the EOF and bubble their production up to the next level.
        // match stack.pop() {
        //     Some(mut builder) => {
        //         let mut pushed = builder.process_eof(py, py_env)?;
        //         while let Some(mut builder) = stack.pop() {
        //             pushed = {
        //                 builder.process_push_from_inner_builder(py, py_env, pushed)?;
        //                 builder.process_eof(py, py_env)?
        //             };
        //         }
        //         // If there were builders, then a new_elem was produced and we need to bubble it up to the next file
        //         top.process_push_from_inner_builder(py, py_env, pushed)?;
        //     }
        //     // If there weren't any builders, we don't need to do anything
        //     None => {}
        // };

        top.finalize(py)
    }
}

/// Holds multiple stacks of builders including an always-present top level builder.
/// Each stack of builders
struct BuilderStacks {
    top: TopLevelDocumentBuilder,
    /// The stacks of builders, one stack per file
    builder_stacks: Vec<Vec<Box<dyn BuildFromTokens>>>,
}
impl BuilderStacks {
    fn new(py: Python) -> PyResult<Self> {
        Ok(Self {
            top: TopLevelDocumentBuilder::new(py)?,
            builder_stacks: vec![vec![]], // Constant condition: there is always at least one builder stack
        })
    }

    fn top_builder(&mut self) -> &mut dyn BuildFromTokens {
        // Loop through builder stacks from end to start, return topmost builder if any is present
        for stack in self.builder_stacks.iter_mut().rev() {
            match stack.last_mut() {
                Some(builder) => return builder.as_mut(),
                None => continue,
            };
        }
        &mut self.top
    }
    fn push_to_current_file(&mut self, new_builder: Box<dyn BuildFromTokens>) {
        self.builder_stacks
            .last_mut()
            .expect("Must always have at least one builder stack")
            .push(new_builder);
    }
    fn pop_from_current_file(&mut self) -> Option<()> {
        self.builder_stacks
            .last_mut()
            .expect("Must always have at least one builder stack")
            .pop()?;
        Some(())
    }
    fn push_subfile(&mut self) {
        self.builder_stacks.push(vec![])
    }
    fn pop_subfile(&mut self) -> Vec<Box<dyn BuildFromTokens>> {
        let stack = self
            .builder_stacks
            .pop()
            .expect("Must always have at least one builder stack");
        if self.builder_stacks.len() == 0 {
            panic!("Popped the last builder stack inside pop_file! ParserStacks must always have at least one stack")
        }
        stack
    }
    fn finalize(mut self) -> (TopLevelDocumentBuilder, Vec<Box<dyn BuildFromTokens>>) {
        let last_stack = self
            .builder_stacks
            .pop()
            .expect("Must always have at least one builder stack");
        if self.builder_stacks.len() > 0 {
            panic!("Called finalize() on BuilderStacks when more than one stack was left - forgot to pop_subfile()?");
        }
        (self.top, last_stack)
    }
}

trait BlockTokenProcessor {
    /// It's common to expect a newline after emitting a block's worth of content
    fn is_expecting_newline(&self) -> bool;
    fn process_expected_newline(&mut self);
    fn process_other_token_when_expecting_newline(&mut self) -> TurnipTextContextlessError;

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
        if self.is_expecting_newline() {
            match tok {
                TTToken::Whitespace(_) => Ok(BuildStatus::Continue),
                TTToken::Newline(_) => {
                    self.process_expected_newline();
                    Ok(BuildStatus::Continue)
                }

                _ => Err(self.process_other_token_when_expecting_newline()),
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

                TTToken::BlockScopeOpen(start_span) => Ok(BuildStatus::StartInnerBuilder(
                    BlockScopeFromTokens::new(py, start_span)?,
                )),

                TTToken::InlineScopeOpen(start_span) => Ok(BuildStatus::StartInnerBuilder(
                    InlineScopeFromTokens::new(py, start_span)?,
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

                TTToken::CodeClose(span, _)
                | TTToken::CodeCloseOwningInline(span, _)
                | TTToken::CodeCloseOwningRaw(span, _, _)
                | TTToken::CodeCloseOwningBlock(span, _) => {
                    Err(InterpError::CodeCloseOutsideCode(span).into())
                }

                TTToken::RawScopeClose(span, _) => {
                    Err(InterpError::RawScopeCloseOutsideRawScope(span).into())
                }

                TTToken::ScopeClose(_) => self.on_close_scope(py, tok, data),

                TTToken::EOF(_) => self.on_eof(py, tok),
            }
        }
    }
}

struct InlineTextState {
    text: String,
    /// pending_whitespace is appended to `text` before new text is added, but can be ignored in certain scenarios.
    ///
    /// e.g. "the" + Whitespace(" ") => ("the", " ") - when the next token is "apple", becomes "the" + " " + "apple"
    /// but for "the" + Whitespace(" ") + Newline, the pending_whitespace is dropped.
    pending_whitespace: Option<String>,
}

trait InlineTokenProcessor {
    fn at_start_of_line(&self) -> bool;
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
                // Swallow whitespace at the start of the line
                if self.at_start_of_line() {
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

            TTToken::InlineScopeOpen(start_span) => {
                self.flush_pending_text(py, true)?;
                Ok(BuildStatus::StartInnerBuilder(InlineScopeFromTokens::new(
                    py, start_span,
                )?))
            }

            TTToken::RawScopeOpen(start_span, n_opening) => {
                self.flush_pending_text(py, true)?;
                Ok(BuildStatus::StartInnerBuilder(RawStringFromTokens::new(
                    start_span, n_opening,
                )))
            }

            // A BlockScopeOpen = (InlineScopeOpen + Newline)
            // so this is a special case of sentence break inside inline scope
            TTToken::BlockScopeOpen(scope_open_span) => {
                Err(InterpError::SentenceBreakInInlineScope {
                    scope_start: scope_open_span,
                }
                .into())
            }
            TTToken::CodeClose(span, _)
            | TTToken::CodeCloseOwningInline(span, _)
            | TTToken::CodeCloseOwningRaw(span, _, _)
            | TTToken::CodeCloseOwningBlock(span, _) => {
                Err(InterpError::CodeCloseOutsideCode(span).into())
            }

            TTToken::RawScopeClose(span, _) => {
                Err(InterpError::RawScopeCloseOutsideRawScope(span).into())
            }
        }
    }
}

struct TopLevelDocumentBuilder {
    /// The current structure of the document, including toplevel content, segments, and the current block stacks (one block stack per included subfile)
    /// TODO remove the block-stack-handling parts from this
    structure: InterimDocumentStructure,
    expect_newline: bool,
}
impl TopLevelDocumentBuilder {
    fn new(py: Python) -> PyResult<Self> {
        Ok(Self {
            structure: InterimDocumentStructure::new(py)?,
            expect_newline: false,
        })
    }

    fn finalize(mut self, py: Python) -> TurnipTextContextlessResult<Py<DocSegment>> {
        self.structure.pop_segments_until_less_than(py, i64::MIN)?;
        self.structure.finalize(py).err_as_internal(py)
    }
}
impl BlockTokenProcessor for TopLevelDocumentBuilder {
    fn is_expecting_newline(&self) -> bool {
        self.expect_newline
    }

    fn process_expected_newline(&mut self) {
        self.expect_newline = false;
    }

    fn process_other_token_when_expecting_newline(&mut self) -> TurnipTextContextlessError {
        todo!("error on non-newline token when newline is expected")
    }

    fn on_close_scope(
        &mut self,
        py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        Err(InterpError::ScopeCloseOutsideScope(tok.token_span()).into())
    }

    // When EOF comes, we don't produce anything to bubble up - there's nothing above us!
    fn on_eof(&mut self, py: Python, tok: TTToken) -> TurnipTextContextlessResult<BuildStatus> {
        Ok(BuildStatus::Continue)
    }
}
impl BuildFromTokens for TopLevelDocumentBuilder {
    // This builder is responsible for spawning lower-level builders
    fn process_token(
        &mut self,
        py: Python,
        py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        self.process_block_level_token(py, tok, data)
    }

    fn process_push_from_inner_builder(
        &mut self,
        py: Python,
        py_env: &PyDict,
        pushed: Option<PushToNextLevel>,
        closing_token: TTToken,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        match pushed {
            Some(PushToNextLevel { from_builder, elem }) => match elem {
                DocElement::Header(header) => {
                    self.structure
                        .push_segment_header(py, header, from_builder.from_span, None)?;
                    Ok(BuildStatus::Continue)
                }
                DocElement::Block(block) => {
                    self.structure.push_to_topmost_block(py, block.as_ref(py))?;
                    Ok(BuildStatus::Continue)
                }
                // If we get an inline, start building a paragraph with it
                DocElement::Inline(inline) => Ok(BuildStatus::StartInnerBuilder(
                    ParagraphFromTokens::new_with_inline(
                        py,
                        inline.as_ref(py),
                        from_builder.from_span,
                    )?,
                )),
                // If we get a raw, convert it to an inline Raw() object and start building a paragraph with it
                DocElement::Raw(data) => {
                    let raw = py_internal_alloc(py, Raw::new_rs(py, data.as_str()))?;
                    Ok(BuildStatus::StartInnerBuilder(
                        ParagraphFromTokens::new_with_inline(
                            py,
                            raw.as_ref(py),
                            from_builder.from_span,
                        )?,
                    ))
                }
            },
            None => Ok(BuildStatus::Continue),
        }
    }
}

struct CommentFromTokens {}
impl CommentFromTokens {
    fn new() -> Box<Self> {
        Box::new(Self {})
    }
}
impl BuildFromTokens for CommentFromTokens {
    fn process_token(
        &mut self,
        py: Python,
        py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        match tok {
            TTToken::Newline(_) | TTToken::EOF(_) => Ok(BuildStatus::Done(None)),
            _ => Ok(BuildStatus::Continue),
        }
    }

    fn process_push_from_inner_builder(
        &mut self,
        py: Python,
        py_env: &PyDict,
        pushed: Option<PushToNextLevel>,
        closing_token: TTToken,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        panic!("CommentFromTokens does not spawn inner builders")
    }
}

struct RawStringFromTokens {
    ctx: BuilderContext,
    n_closing: usize,
    raw_data: String,
}
impl RawStringFromTokens {
    fn new(start_span: ParseSpan, n_opening: usize) -> Box<Self> {
        Box::new(Self {
            ctx: BuilderContext::new("RawString", start_span),
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
        match tok {
            TTToken::RawScopeClose(_, given_closing) if given_closing == self.n_closing => {
                self.ctx.try_extend(&tok);
                Ok(BuildStatus::Done(Some(self.ctx.make(DocElement::Raw(
                    std::mem::take(&mut self.raw_data),
                )))))
            }
            TTToken::EOF(_) => Err(InterpError::EndedInsideRawScope {
                raw_scope_start: self.ctx.from_span,
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
        closing_token: TTToken,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        panic!("RawStringFromTokens does not spawn inner builders")
    }
}

struct BlockScopeFromTokens {
    ctx: BuilderContext,
    expect_newline: bool, // Set to true after pushing Blocks and Headers into the InterimDocumentStructure
    block_scope: Py<BlockScope>,
}
impl BlockScopeFromTokens {
    fn new(py: Python, start_span: ParseSpan) -> TurnipTextContextlessResult<Box<Self>> {
        Ok(Box::new(Self {
            ctx: BuilderContext::new("BlockScope", start_span),
            expect_newline: false,
            block_scope: py_internal_alloc(py, BlockScope::new_empty(py))?,
        }))
    }
}
impl BlockTokenProcessor for BlockScopeFromTokens {
    fn is_expecting_newline(&self) -> bool {
        self.expect_newline
    }

    fn process_expected_newline(&mut self) {
        self.expect_newline = false;
    }

    fn process_other_token_when_expecting_newline(&mut self) -> TurnipTextContextlessError {
        todo!("Error when expecting newline")
    }

    fn on_close_scope(
        &mut self,
        py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        if !self.ctx.try_extend(&tok) {
            todo!("error to say 'closing block scope from different file'");
        }
        Ok(BuildStatus::Done(Some(self.ctx.make(DocElement::Block(
            PyTcRef::of_unchecked(self.block_scope.as_ref(py)),
        )))))
    }

    fn on_eof(&mut self, py: Python, tok: TTToken) -> TurnipTextContextlessResult<BuildStatus> {
        Err(InterpError::EndedInsideScope {
            scope_start: self.ctx.from_span,
        }
        .into())
    }
}
impl BuildFromTokens for BlockScopeFromTokens {
    fn process_token(
        &mut self,
        py: Python,
        py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        self.process_block_level_token(py, tok, data)
    }

    fn process_push_from_inner_builder(
        &mut self,
        py: Python,
        py_env: &PyDict,
        pushed: Option<PushToNextLevel>,
        closing_token: TTToken,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        match pushed {
            Some(PushToNextLevel { from_builder, elem }) => match elem {
                DocElement::Header(_) => Err(InterpError::DocSegmentHeaderMidScope {
                    code_span: from_builder.from_span,
                    block_close_span: None,
                    enclosing_scope_start: self.ctx.from_span,
                }
                .into()),
                DocElement::Block(block) => {
                    self.block_scope
                        .borrow_mut(py)
                        .push_block(block.as_ref(py))
                        .err_as_internal(py)?;
                    // If the block in the previous scope ended with a newline (i.e. it was a paragraph)
                    // then don't expect another newline afterwards.
                    // Otherwise, do.
                    // TODO Test this
                    // TODO implement this in top-level
                    if let TTToken::Newline(_) = closing_token {
                        self.expect_newline = false;
                    } else {
                        self.expect_newline = true;
                    }
                    Ok(BuildStatus::Continue)
                }
                // If we get an inline, start building a paragraph inside this block-scope with it
                DocElement::Inline(inline) => Ok(BuildStatus::StartInnerBuilder(
                    ParagraphFromTokens::new_with_inline(
                        py,
                        inline.as_ref(py),
                        from_builder.from_span,
                    )?,
                )),
                // If we get a raw, convert it to an inline Raw() object and start building a paragraph inside this block-scope with it
                DocElement::Raw(data) => {
                    let raw = py_internal_alloc(py, Raw::new_rs(py, data.as_str()))?;
                    Ok(BuildStatus::StartInnerBuilder(
                        ParagraphFromTokens::new_with_inline(
                            py,
                            raw.as_ref(py),
                            from_builder.from_span,
                        )?,
                    ))
                }
            },
            None => Ok(BuildStatus::Continue),
        }
    }
}

// TODO this and InlineScopeFromTokens could share more w.r.t. text
struct ParagraphFromTokens {
    ctx: BuilderContext,
    para: Py<Paragraph>,
    // TODO test whitespace after the start of an inline scope
    start_of_line: bool,
    current_building_text: Option<InlineTextState>,
    current_sentence: Py<Sentence>,
}
impl ParagraphFromTokens {
    fn new_with_inline(
        py: Python,
        inline: &PyAny,
        inline_span: ParseSpan,
    ) -> TurnipTextContextlessResult<Box<Self>> {
        let current_sentence = py_internal_alloc(py, Sentence::new_empty(py))?;
        current_sentence
            .borrow_mut(py)
            .push_inline(inline)
            .err_as_internal(py)?;
        Ok(Box::new(Self {
            ctx: BuilderContext::new("Paragraph", inline_span),
            para: py_internal_alloc(py, Paragraph::new_empty(py))?,
            start_of_line: false,
            current_building_text: None,
            current_sentence,
        }))
    }
    fn new_with_starting_text(
        py: Python,
        text: &str,
        text_span: ParseSpan,
    ) -> TurnipTextContextlessResult<Box<Self>> {
        Ok(Box::new(Self {
            ctx: BuilderContext::new("Paragraph", text_span),
            para: py_internal_alloc(py, Paragraph::new_empty(py))?,
            start_of_line: false,
            current_building_text: Some(InlineTextState {
                text: text.to_string(),
                pending_whitespace: None,
            }),
            current_sentence: py_internal_alloc(py, Sentence::new_empty(py))?,
        }))
    }
    /// Replace self.current_building_text with None. If it was Some() before, take the text component (not the pending whitespace) put it into a Text() inline object, and push that object into the inline scope.
    fn fold_current_text_into_sentence(
        &mut self,
        py: Python,
        include_whitespace: bool,
    ) -> TurnipTextContextlessResult<()> {
        match std::mem::take(&mut self.current_building_text) {
            Some(mut current_text) => {
                if include_whitespace {
                    if let Some(w) = current_text.pending_whitespace {
                        current_text.text.push_str(&w)
                    }
                }
                let current_text = py_internal_alloc(py, Text::new_rs(py, &current_text.text))?;
                self.current_sentence
                    .borrow_mut(py)
                    .push_inline(current_text.as_ref(py))
                    .err_as_internal(py)
            }
            None => Ok(()),
        }
    }
}
impl InlineTokenProcessor for ParagraphFromTokens {
    fn at_start_of_line(&self) -> bool {
        self.start_of_line
    }
    fn clear_pending_whitespace(&mut self) {
        match &mut self.current_building_text {
            Some(InlineTextState {
                text: _,
                pending_whitespace,
            }) => *pending_whitespace = None,
            None => {}
        }
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
        let text_content = tok.stringify_escaped(data);
        self.start_of_line = false;
        match &mut self.current_building_text {
            Some(InlineTextState {
                text,
                pending_whitespace,
            }) => {
                if let Some(w) = std::mem::take(pending_whitespace) {
                    text.push_str(&w)
                }
                text.push_str(text_content)
            }
            None => {
                self.current_building_text = Some(InlineTextState {
                    text: text_content.to_string(),
                    pending_whitespace: None,
                })
            }
        };
        Ok(BuildStatus::Continue)
    }

    fn on_midline_whitespace(
        &mut self,
        py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        let whitespace_content = tok.stringify_escaped(data);
        match &mut self.current_building_text {
            Some(InlineTextState {
                text,
                pending_whitespace,
            }) => {
                // Push new whitespace into pending_whitespace
                // TODO this means you can be text=" ", pending_whitespace=" " at the same time. weird.
                match pending_whitespace {
                    Some(w) => w.push_str(whitespace_content),
                    None => *pending_whitespace = Some(whitespace_content.to_string()),
                }
            }
            // Don't skip whitespace when we're mid-line - even if we aren't building text!
            None => {
                self.current_building_text = Some(InlineTextState {
                    text: whitespace_content.to_string(),
                    pending_whitespace: None,
                })
            }
        };
        Ok(BuildStatus::Continue)
    }

    fn on_newline(&mut self, py: Python, tok: TTToken) -> TurnipTextContextlessResult<BuildStatus> {
        // Text is already folded into the sentence
        if self.start_of_line {
            if !self.ctx.try_extend(&tok) {
                // TODO should this really be an error??
                todo!("error to say 'closing paragraph from different file'");
            }
            Ok(BuildStatus::Done(Some(self.ctx.make(DocElement::Block(
                PyTcRef::of_unchecked(self.para.as_ref(py)),
            )))))
        } else {
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
            // We're now at the start of the line
            self.start_of_line = true;
            Ok(BuildStatus::Continue)
        }
    }

    fn on_eof(&mut self, py: Python, tok: TTToken) -> TurnipTextContextlessResult<BuildStatus> {
        if !self.start_of_line {
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
        if !self.ctx.try_extend(&tok) {
            // TODO should this really be an error??
            todo!("error to say 'closing paragraph from different file'");
        }
        Ok(BuildStatus::Done(Some(self.ctx.make(DocElement::Block(
            PyTcRef::of_unchecked(self.para.as_ref(py)),
        )))))
    }

    fn on_close_scope(
        &mut self,
        py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        // TODO this will catch a closing brace on a line just under a paragraph, when the paragraph hasn't ended yet. Add a test case.
        // This isn't an error in the old version.
        todo!("error: closing scope inside a paragraph when no inline scopes are open")
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
        closing_token: TTToken,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        self.start_of_line = false;
        match pushed {
            Some(PushToNextLevel { from_builder, elem }) => match elem {
                // Can't get a header or a block in an inline scope
                DocElement::Header(_) => {
                    return Err(InterpError::DocSegmentHeaderMidPara {
                        code_span: from_builder.from_span,
                    }
                    .into())
                }
                DocElement::Block(_) => {
                    return Err(InterpError::BlockCodeMidPara {
                        code_span: from_builder.from_span,
                    }
                    .into())
                }
                // If we get an inline, shove it in
                DocElement::Inline(inline) => {
                    self.current_sentence
                        .borrow_mut(py)
                        .push_inline(inline.as_ref(py))
                        .err_as_internal(py)?;
                }
                // If we get a raw, convert it to an inline Raw() object and shove it in
                DocElement::Raw(data) => {
                    let raw = py_internal_alloc(py, Raw::new_rs(py, data.as_str()))?;
                    self.current_sentence
                        .borrow_mut(py)
                        .push_inline(raw.as_ref(py))
                        .err_as_internal(py)?;
                    {}
                }
            },
            None => {}
        }
        match closing_token {
            TTToken::Newline(_) => self.on_newline(py, closing_token),
            TTToken::EOF(_) => self.on_eof(py, closing_token),
            _ => Ok(BuildStatus::Continue),
        }
    }
}

struct InlineScopeFromTokens {
    ctx: BuilderContext,
    inline_scope: Py<InlineScope>,
    // TODO test whitespace after the start of an inline scope
    start_of_line: bool,
    current_building_text: Option<InlineTextState>,
}
impl InlineScopeFromTokens {
    fn new(py: Python, start_span: ParseSpan) -> TurnipTextContextlessResult<Box<Self>> {
        Ok(Box::new(Self {
            ctx: BuilderContext::new("BlockScope", start_span),
            start_of_line: true,
            inline_scope: py_internal_alloc(py, InlineScope::new_empty(py))?,
            current_building_text: None,
        }))
    }
    /// Replace self.current_building_text with None. If it was Some() before, take the text component (not the pending whitespace) put it into a Text() inline object, and push that object into the inline scope.
    fn fold_current_text_into_scope(
        &mut self,
        py: Python,
        include_whitespace: bool,
    ) -> TurnipTextContextlessResult<()> {
        match std::mem::take(&mut self.current_building_text) {
            Some(mut current_text) => {
                if include_whitespace {
                    if let Some(w) = current_text.pending_whitespace {
                        current_text.text.push_str(&w)
                    }
                }
                let current_text = py_internal_alloc(py, Text::new_rs(py, &current_text.text))?;
                self.inline_scope
                    .borrow_mut(py)
                    .push_inline(current_text.as_ref(py))
                    .err_as_internal(py)
            }
            None => Ok(()),
        }
    }
}
impl InlineTokenProcessor for InlineScopeFromTokens {
    fn at_start_of_line(&self) -> bool {
        self.start_of_line
    }
    fn clear_pending_whitespace(&mut self) {
        match &mut self.current_building_text {
            Some(InlineTextState {
                text: _,
                pending_whitespace,
            }) => *pending_whitespace = None,
            None => {}
        }
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
        self.start_of_line = false;
        match &mut self.current_building_text {
            Some(InlineTextState {
                text,
                pending_whitespace,
            }) => {
                if let Some(w) = std::mem::take(pending_whitespace) {
                    text.push_str(&w)
                }
                text.push_str(tok.stringify_escaped(data))
            }
            None => {
                self.current_building_text = Some(InlineTextState {
                    text: tok.stringify_escaped(data).to_string(),
                    pending_whitespace: None,
                })
            }
        };
        Ok(BuildStatus::Continue)
    }

    fn on_midline_whitespace(
        &mut self,
        py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        self.start_of_line = false;
        match &mut self.current_building_text {
            Some(InlineTextState {
                text,
                pending_whitespace,
            }) => {
                if let Some(w) = std::mem::take(pending_whitespace) {
                    text.push_str(&w)
                }
                text.push_str(tok.stringify_escaped(data))
            }
            // Don't skip whitespace when we're mid-line - even if we aren't building text!
            None => {
                self.current_building_text = Some(InlineTextState {
                    text: tok.stringify_escaped(data).to_string(),
                    pending_whitespace: None,
                })
            }
        };
        Ok(BuildStatus::Continue)
    }

    fn on_newline(&mut self, py: Python, tok: TTToken) -> TurnipTextContextlessResult<BuildStatus> {
        Err(InterpError::SentenceBreakInInlineScope {
            scope_start: self.ctx.from_span,
        }
        .into())
    }

    fn on_close_scope(
        &mut self,
        py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        // pending text has already been folded in
        if !self.ctx.try_extend(&tok) {
            todo!("error to say 'closing inline scope from different file'");
        }
        Ok(BuildStatus::Done(Some(self.ctx.make(DocElement::Inline(
            PyTcRef::of_unchecked(self.inline_scope.as_ref(py)),
        )))))
    }

    fn on_eof(&mut self, py: Python, tok: TTToken) -> TurnipTextContextlessResult<BuildStatus> {
        Err(InterpError::EndedInsideScope {
            scope_start: self.ctx.from_span,
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
        closing_token: TTToken,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        self.start_of_line = false;
        // Before we do anything else, push the current text into the scope including the whitespace between the text and the newly pushed item
        self.fold_current_text_into_scope(py, true)?;
        match pushed {
            Some(PushToNextLevel { from_builder, elem }) => match elem {
                // Can't get a header or a block in an inline scope
                DocElement::Header(_) => {
                    return Err(InterpError::DocSegmentHeaderMidPara {
                        code_span: from_builder.from_span,
                    }
                    .into())
                }
                DocElement::Block(_) => {
                    return Err(InterpError::BlockCodeMidPara {
                        code_span: from_builder.from_span,
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
                // If we get a raw, convert it to an inline Raw() object and shove it in
                DocElement::Raw(data) => {
                    let raw = py_internal_alloc(py, Raw::new_rs(py, data.as_str()))?;
                    self.inline_scope
                        .borrow_mut(py)
                        .push_inline(raw.as_ref(py))
                        .err_as_internal(py)?;
                }
            },
            None => {}
        };
        match closing_token {
            TTToken::Newline(_) => self.on_newline(py, closing_token),
            TTToken::EOF(_) => self.on_eof(py, closing_token),
            _ => Ok(BuildStatus::Continue),
        }
    }
}

struct CodeFromTokens {
    start_span: ParseSpan,
    n_closing: usize,
    code: String,
}
impl CodeFromTokens {
    fn new(start_span: ParseSpan, n_opening: usize) -> Box<Self> {
        Box::new(Self {
            start_span,
            n_closing: n_opening,
            code: String::new(),
        })
    }
}
impl BuildFromTokens for CodeFromTokens {
    fn process_token(
        &mut self,
        py: Python,
        py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        match eval_brackets(
            data,
            tok,
            &mut self.code,
            &self.start_span,
            self.n_closing,
            py,
            py_env,
        )? {
            Some((evaled_obj, code_span)) => {
                let ctx = BuilderContext::new("Code", code_span);
                match evaled_obj {
                    EvalBracketResult::NeededBlockBuilder(_) => todo!(),
                    EvalBracketResult::NeededInlineBuilder(_) => todo!(),
                    EvalBracketResult::NeededRawBuilder(_, _) => todo!(),
                    EvalBracketResult::DocSegmentHeader(header) => Ok(BuildStatus::Done(Some(
                        ctx.make(DocElement::Header(header)),
                    ))),
                    EvalBracketResult::Block(block) => {
                        Ok(BuildStatus::Done(Some(ctx.make(DocElement::Block(block)))))
                    }
                    EvalBracketResult::Inline(inline) => Ok(BuildStatus::Done(Some(
                        ctx.make(DocElement::Inline(inline)),
                    ))),
                    EvalBracketResult::TurnipTextSource(src) => Ok(BuildStatus::NewSource(src)),
                    EvalBracketResult::PyNone => Ok(BuildStatus::Done(None)),
                }
            }
            None => Ok(BuildStatus::Continue),
        }
    }

    fn process_push_from_inner_builder(
        &mut self,
        py: Python,
        py_env: &PyDict,
        pushed: Option<PushToNextLevel>,
        closing_token: TTToken,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        todo!()
    }
}

fn py_internal_alloc<T: PyClass>(
    py: Python<'_>,
    value: impl Into<PyClassInitializer<T>>,
) -> TurnipTextContextlessResult<Py<T>> {
    Py::new(py, value).err_as_internal(py)
}