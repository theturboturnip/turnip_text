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

use std::{cell::RefCell, rc::Rc};

use pyo3::{types::PyDict, Py, PyAny, PyClass, PyClassInitializer, PyResult, Python};

use crate::{
    error::{TurnipTextContextlessError, TurnipTextContextlessResult},
    interpreter::{
        python::typeclass::PyTcUnionRef, BlockScopeBuilder, InlineScopeBuilder, RawScopeBuilder,
    },
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
    /// On rare occasions it is necessary to bubble the token up to the next builder as well as the finished item.
    /// This applies to
    /// - scope-closes at the start of the line when in paragraph-mode - these scope closes are clearly intended for an enclosing block scope, so the paragraph should finish and the containing builder should handle the scope-close
    /// - EOFs in all non-error cases, because those should bubble up through the file
    /// - newlines at the end of comments, because those still signal the end of sentence in inline mode and count as a blank line in block mode
    /// None of these are allowed to cross file boundaries. Scope-closes crossing file boundaries invoke a ScopeCloseOutsideScope error. EOFs and newlines are silently ignored.
    /// TODO ensure the above :)
    DoneAndReprocessToken(Option<PushToNextLevel>),
    Continue,
    StartInnerBuilder(Rc<RefCell<dyn BuildFromTokens>>),
    DoneAndNewSource(TurnipTextSource),
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
    fn try_extend(&mut self, span: &ParseSpan) -> bool {
        if span.file_idx() == self.from_span.file_idx() {
            self.from_span = self.from_span.combine(span);
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
    /// This will usually receive tokens from the same file it was created in, unless a source file is opened within it
    /// in which case it will receive the top-level tokens from that file too except EOF.
    ///
    /// Note: this means if an impl doesn't override [allow_emitting_new_sources_inside] to true then it will always receive tokens from the same file.
    ///
    /// When receiving any token from an inner file, this function must return either an error, [BuildStatus::Continue], or [BuildStatus::StartInnerBuilder]. Other responses would result in modifying the outer file due to the contents of the inner file, and are not allowed.
    ///
    /// When receiving [TTToken::EOF] this function must return either an error or [BuildStatus::DoneAndReprocessToken]. Other responses are not allowed.
    ///
    /// TODO prove this contract in the comments on block-scope builders.
    fn process_token(
        &mut self,
        py: Python,
        py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus>;
    /// May only return an error, [BuildStatus::Continue], [BuildStatus::StartInnerBuilder], or [BuildStatus::Done].
    fn process_push_from_inner_builder(
        &mut self,
        py: Python,
        py_env: &PyDict,
        pushed: Option<PushToNextLevel>,
        // closing_tok: TTToken,
    ) -> TurnipTextContextlessResult<BuildStatus>;
    // Make it opt-in to allow emitting new source files
    fn allow_emitting_new_sources_inside(&self) -> bool {
        false
    }
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
            match self
                .builders
                .top_stack()
                .process_token(py, py_env, tok, data)?
            {
                None => continue,
                Some(TurnipTextSource { name, contents }) => {
                    return Ok(InterpreterFileAction::FileInserted { name, contents })
                }
            }
        }
        Ok(InterpreterFileAction::FileEnded)
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
        let stack = self.builders.pop_subfile();
        // The EOF token from the end of the file should have bubbled everything out.
        // TODO it is possible to break this if Paragraph receives EOF inside BlockScope - Paragraph will not bubble EOF out to blockscope for error
        // TODO if the Paragraph does bubble out the EOF then it can bubble it out through files
        assert!(stack.stack.is_empty());
        Ok(())
    }

    pub fn finalize(
        self,
        py: Python,
        py_env: &PyDict,
    ) -> TurnipTextContextlessResult<Py<DocSegment>> {
        let rc_refcell_top = self.builders.finalize();
        match Rc::try_unwrap(rc_refcell_top) {
            Err(_) => panic!("Shouldn't have any other stacks holding references to this"),
            Ok(refcell_top) => refcell_top.into_inner().finalize(py),
        }
    }
}

struct FileBuilderStack {
    top: Rc<RefCell<dyn BuildFromTokens>>,
    /// The stack of builders created inside this file.
    stack: Vec<Rc<RefCell<dyn BuildFromTokens>>>,
}
impl FileBuilderStack {
    fn new(top: Rc<RefCell<dyn BuildFromTokens>>) -> Self {
        Self { top, stack: vec![] }
    }

    fn curr_top(&self) -> &Rc<RefCell<dyn BuildFromTokens>> {
        match self.stack.last() {
            None => &self.top,
            Some(top) => top,
        }
    }

    fn process_token(
        &mut self,
        py: Python,
        py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<Option<TurnipTextSource>> {
        // If processing an EOF we need to flush all builders in the stack and not pass through tokens to self.top()
        if let TTToken::EOF(_) = &tok {
            loop {
                let top = match self.stack.pop() {
                    None => break,
                    Some(top) => top,
                };
                let action = top.borrow_mut().process_token(py, py_env, tok, data)?;
                match action {
                    BuildStatus::DoneAndReprocessToken(pushed) => {
                        self.push_to_top_builder(py, py_env, pushed)?
                    }
                    _ => unreachable!("builder returned a BuildStatus that wasn't DoneAndReprocessToken in response to an EOF")
                }
            }
            Ok(None)
        } else {
            // The token is not EOF, so we are allowed to pass the token through to self.top.
            // If there are no builders, we can pass the token to the top builder and see what it says.
            if self.stack.is_empty() {
                let action = self.top.borrow_mut().process_token(py, py_env, tok, data)?;
                match action {
                    BuildStatus::Done(_)
                    | BuildStatus::DoneAndReprocessToken(_)
                    | BuildStatus::DoneAndNewSource(_) => {
                        unreachable!("builder for previous file returned a Done* when presented with a token for an inner file")
                    }
                    BuildStatus::StartInnerBuilder(builder) => self.stack.push(builder),
                    BuildStatus::Continue => {}
                };
                Ok(None)
            } else {
                // If there are builders, pass the token to the topmost one.
                // The token may bubble out if the builder returns DoneAndReprocessToken, so loop to support that case and break out with returns otherwise.
                loop {
                    let action = self
                        .curr_top()
                        .borrow_mut()
                        .process_token(py, py_env, tok, data)?;
                    match action {
                        BuildStatus::Continue => return Ok(None),
                        BuildStatus::StartInnerBuilder(builder) => {
                            self.stack.push(builder);
                            return Ok(None);
                        }
                        BuildStatus::Done(pushed) => {
                            self.stack.pop().expect("self.curr_top() returned Done => it must be processing a token from the file it was created in => it must be on the stack => the stack must have something on it.");
                            self.push_to_top_builder(py, py_env, pushed)?;
                            return Ok(None);
                        }
                        BuildStatus::DoneAndReprocessToken(pushed) => {
                            self.stack.pop().expect("self.curr_top() returned Done => it must be processing a token from the file it was created in => it must be on the stack => the stack must have something on it.");
                            self.push_to_top_builder(py, py_env, pushed)?;
                            if self.stack.is_empty() {
                                // The token is bubbling up to the next file - stop that from happening!
                                match tok {
                                    // ScopeClose bubbling out => breaking out into a subfile
                                    TTToken::ScopeClose(span) => return Err(InterpError::ScopeCloseOutsideScope(span).into()),
                                    TTToken::Newline(_) => return Ok(None),
                                    _ => unreachable!("The only cases where a token should bubble out are ScopeClose, Newline, and EOF"),
                                }
                            } else {
                                // Don't return, keep going through the loop to bubble the token up to the next builder from this file
                            }
                        }
                        BuildStatus::DoneAndNewSource(src) => {
                            self.stack.pop().expect("self.curr_top() returned Done => it must be processing a token from the file it was created in => it must be on the stack => the stack must have something on it.");
                            return Ok(Some(src));
                        }
                    }
                }
            }
        }
    }

    fn push_to_top_builder(
        &mut self,
        py: Python,
        py_env: &PyDict,
        pushed: Option<PushToNextLevel>,
    ) -> TurnipTextContextlessResult<()> {
        let action = self
            .curr_top()
            .borrow_mut()
            .process_push_from_inner_builder(py, py_env, pushed)?;
        match action {
            BuildStatus::Continue => Ok(()),
            BuildStatus::Done(new_pushed) => {
                self.stack.pop().expect("self.curr_top() returned Done => it must be processing a token from the file it was created in => it must be on the stack => the stack must have something on it.");
                self.push_to_top_builder(py, py_env, new_pushed)
            }
            BuildStatus::StartInnerBuilder(builder) => {
                self.stack.push(builder);
                Ok(())
            },
            BuildStatus::DoneAndReprocessToken(_) | BuildStatus::DoneAndNewSource(_) => unreachable!("process_push_from_inner_builder may not return DoneAndReprocessToken or DoneAndNewSource."),
        }
    }
}

/// Holds multiple stacks of builders including an always-present top level builder.
/// Each stack of builders
struct BuilderStacks {
    top: Rc<RefCell<TopLevelDocumentBuilder>>,
    /// The stacks of builders, one stack per file
    builder_stacks: Vec<FileBuilderStack>,
}
impl BuilderStacks {
    fn new(py: Python) -> PyResult<Self> {
        let top = TopLevelDocumentBuilder::new(py)?;
        Ok(Self {
            builder_stacks: vec![FileBuilderStack::new(top.clone())], // Constant condition: there is always at least one builder stack
            top,
        })
    }

    fn top_stack(&mut self) -> &mut FileBuilderStack {
        self.builder_stacks
            .last_mut()
            .expect("Must always have at least one builder stack")
    }

    fn push_subfile(&mut self) {
        let topmost_builder_enclosing_new_file = self.top_stack().curr_top().clone();
        self.builder_stacks
            .push(FileBuilderStack::new(topmost_builder_enclosing_new_file))
    }
    fn pop_subfile(&mut self) -> FileBuilderStack {
        let stack = self
            .builder_stacks
            .pop()
            .expect("Must always have at least one builder stack");
        assert!(stack.stack.is_empty());
        if self.builder_stacks.len() == 0 {
            panic!("Popped the last builder stack inside pop_file! ParserStacks must always have at least one stack")
        }
        stack
    }

    fn finalize(mut self) -> Rc<RefCell<TopLevelDocumentBuilder>> {
        let last_stack = self
            .builder_stacks
            .pop()
            .expect("Must always have at least one builder stack");
        assert!(last_stack.stack.is_empty());
        if self.builder_stacks.len() > 0 {
            panic!("Called finalize() on BuilderStacks when more than one stack was left - forgot to pop_subfile()?");
        }
        self.top
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
    fn new(py: Python) -> PyResult<Rc<RefCell<Self>>> {
        Ok(rc_refcell(Self {
            structure: InterimDocumentStructure::new(py)?,
            expect_newline: false,
        }))
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
    fn allow_emitting_new_sources_inside(&self) -> bool {
        true
    }

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
        // closing_token: TTToken,
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
    fn new() -> Rc<RefCell<Self>> {
        rc_refcell(Self {})
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
            TTToken::Newline(_) | TTToken::EOF(_) => Ok(BuildStatus::DoneAndReprocessToken(None)),
            _ => Ok(BuildStatus::Continue),
        }
    }

    fn process_push_from_inner_builder(
        &mut self,
        py: Python,
        py_env: &PyDict,
        pushed: Option<PushToNextLevel>,
        // closing_token: TTToken,
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
    fn new(start_span: ParseSpan, n_opening: usize) -> Rc<RefCell<Self>> {
        rc_refcell(Self {
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
                self.ctx.try_extend(&tok.token_span());
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
        // closing_token: TTToken,
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
    fn new(py: Python, start_span: ParseSpan) -> TurnipTextContextlessResult<Rc<RefCell<Self>>> {
        Ok(rc_refcell(Self {
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
        if !self.ctx.try_extend(&tok.token_span()) {
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
    fn allow_emitting_new_sources_inside(&self) -> bool {
        true
    }

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
        // closing_token: TTToken,
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
    ) -> TurnipTextContextlessResult<Rc<RefCell<Self>>> {
        let current_sentence = py_internal_alloc(py, Sentence::new_empty(py))?;
        current_sentence
            .borrow_mut(py)
            .push_inline(inline)
            .err_as_internal(py)?;
        Ok(rc_refcell(Self {
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
    ) -> TurnipTextContextlessResult<Rc<RefCell<Self>>> {
        Ok(rc_refcell(Self {
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
            if !self.ctx.try_extend(&tok.token_span()) {
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
        if !self.ctx.try_extend(&tok.token_span()) {
            // TODO should this really be an error??
            todo!("error to say 'closing paragraph from different file'");
        }
        // TODO should this bubble up the EOF? surely yes, but it should stop at the file boundary
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
        // If the closing brace is at the start of the line, it must be for block-scope and we can assume there won't be text afterwards.
        // End the paragraph, and tell the scope above us in the hierarchy to handle the scope close.
        if self.start_of_line {
            if !self.ctx.try_extend(&tok.token_span()) {
                // TODO should this really be an error??
                todo!("error to say 'closing paragraph from different file'");
            }
            Ok(BuildStatus::DoneAndReprocessToken(Some(self.ctx.make(
                DocElement::Block(PyTcRef::of_unchecked(self.para.as_ref(py))),
            ))))
        } else {
            todo!("error: closing scope inside a paragraph when no inline scopes are open")
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
        Ok(BuildStatus::Continue)
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
    fn new(py: Python, start_span: ParseSpan) -> TurnipTextContextlessResult<Rc<RefCell<Self>>> {
        Ok(rc_refcell(Self {
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
        if !self.ctx.try_extend(&tok.token_span()) {
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
        // closing_token: TTToken,
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
        Ok(BuildStatus::Continue)
    }
}

struct CodeFromTokens {
    start_span: ParseSpan,
    n_closing: usize,
    code: String,
    builder: Option<EvalBracketResult>,
}
impl CodeFromTokens {
    fn new(start_span: ParseSpan, n_opening: usize) -> Rc<RefCell<Self>> {
        rc_refcell(Self {
            start_span,
            n_closing: n_opening,
            code: String::new(),
            builder: None,
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
        assert!(self.builder.is_none(), "If the builder object is Some then we've already eval-d code and are waiting for the scope with the stuff-to-build to close. We shouldn't be processing tokens.");
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
                    EvalBracketResult::NeededBlockBuilder(_) => {
                        self.builder = Some(evaled_obj);
                        Ok(BuildStatus::StartInnerBuilder(BlockScopeFromTokens::new(
                            py, code_span,
                        )?))
                    }
                    EvalBracketResult::NeededInlineBuilder(_) => {
                        self.builder = Some(evaled_obj);
                        Ok(BuildStatus::StartInnerBuilder(InlineScopeFromTokens::new(
                            py, code_span,
                        )?))
                    }
                    EvalBracketResult::NeededRawBuilder(_, n_opening) => {
                        self.builder = Some(evaled_obj);
                        Ok(BuildStatus::StartInnerBuilder(RawStringFromTokens::new(
                            code_span, n_opening,
                        )))
                    }
                    EvalBracketResult::DocSegmentHeader(header) => Ok(BuildStatus::Done(Some(
                        ctx.make(DocElement::Header(header)),
                    ))),
                    EvalBracketResult::Block(block) => {
                        Ok(BuildStatus::Done(Some(ctx.make(DocElement::Block(block)))))
                    }
                    EvalBracketResult::Inline(inline) => Ok(BuildStatus::Done(Some(
                        ctx.make(DocElement::Inline(inline)),
                    ))),
                    EvalBracketResult::TurnipTextSource(src) => {
                        Ok(BuildStatus::DoneAndNewSource(src))
                    }
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
        // closing_token: TTToken,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        let builder = std::mem::take(&mut self.builder)
            .expect("Should only get a result from an inner builder if self.builder is populated");
        let pushed = pushed.expect("Should never get a built None - CodeFromTokens only spawns BlockScopeFromTokens, InlineScopeFromTokens, RawScopeFromTokens none of which return None.");
        let mut ctx = BuilderContext::new("Code", self.start_span);
        ctx.try_extend(&pushed.from_builder.from_span);
        match (builder, pushed.elem) {
            (EvalBracketResult::NeededBlockBuilder(builder), DocElement::Block(blocks)) => {
                let built =
                    BlockScopeBuilder::call_build_from_blocks(py, builder, blocks.as_ref(py))
                        .err_as_internal(py)?;
                match built {
                    Some(PyTcUnionRef::A(block)) => {
                        Ok(BuildStatus::Done(Some(ctx.make(DocElement::Block(block)))))
                    }
                    Some(PyTcUnionRef::B(header)) => Ok(BuildStatus::Done(Some(
                        ctx.make(DocElement::Header(header)),
                    ))),
                    None => Ok(BuildStatus::Done(None)),
                }
            }
            (EvalBracketResult::NeededInlineBuilder(builder), DocElement::Inline(inlines)) => {
                let built =
                    InlineScopeBuilder::call_build_from_inlines(py, builder, inlines.as_ref(py))
                        .err_as_internal(py)?;
                // match built {
                //     Some(PyTcUnionRef::A(block)) => todo!(),
                //     Some(PyTcUnionRef::B(header)) => todo!(),
                //     None => todo!(),
                // }
                Ok(BuildStatus::Done(Some(ctx.make(DocElement::Inline(built)))))
            }
            (EvalBracketResult::NeededRawBuilder(builder, _), DocElement::Raw(raw)) => {
                let built =
                    RawScopeBuilder::call_build_from_raw(py, &builder, &raw).err_as_internal(py)?;
                match built {
                    PyTcUnionRef::A(inline) => Ok(BuildStatus::Done(Some(
                        ctx.make(DocElement::Inline(inline)),
                    ))),
                    PyTcUnionRef::B(block) => {
                        Ok(BuildStatus::Done(Some(ctx.make(DocElement::Block(block)))))
                    }
                }
            }
            _ => unreachable!("Invalid combination of requested and actual built element types"),
        }
    }
}

fn py_internal_alloc<T: PyClass>(
    py: Python<'_>,
    value: impl Into<PyClassInitializer<T>>,
) -> TurnipTextContextlessResult<Py<T>> {
    Py::new(py, value).err_as_internal(py)
}

fn rc_refcell<T>(t: T) -> Rc<RefCell<T>> {
    Rc::new(RefCell::new(t))
}
