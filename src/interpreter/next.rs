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
//! Each file has a separate [FileBuilderStack], which falls back to the topmost builder in the previous file or the topmost builder of the whole document if no containing files have builders.
//! It's possible to have a tall stack of files where no file is using a builder - e.g. if you have file A include
//! file B include file C include file D, and you just write paragraphs, files A through C will have empty builder stacks while file D is being processed and paragraphs from file D will bubble right up to the top-level document.
//!
//! Previously I hoped to introduce strict newline handling, as the current syntax allows separate blocks to be emitted on the same line (e.g. two eval-bracket-pairs can emit Block on the same line) or on directly adjacent lines (e.g. a block scope open directly under a paragraph, or a paragraph starting directly under an eval-bracket emitting Block).
//! This logic would have to be implemented inside a Builder, and it would need to be checked at the top level of *each* file.
//! This cannot be done with the above structure, unless you allow newlines to cross file borders and affect the builder for the enclosing file.
//! That would mean the correctness of the *enclosing* file would be dependent on the *enclosed* file and vice versa - the enclosed file would inherit newline state from the enclosing file, and the enclosing file would inherit newline state from after the enclosed file finishes.
//! I do not like this, and prefer to keep the correctness of files independent while keeping the wackier possible syntax.
//!
//! TODO the newline syntax I want can be established as
//! - after eval-bracket-emitting-block-or-header-or-none, a newline must be seen before any content
//! - after closing a block-level block scope (i.e. ignoring block scopes used as arguments to code producing inline), a newline must be seen before any content
//! There's no need to worry about this with paragraphs (double newline required to end the paragraph) and block scope opens/closes (extra newlines between opening new block scopes seems superfluous, opening a block scope requires a newline, block scopes can't be closed mid paragraph)
//!
//! TODO rename the MidPara errors because they could be used in an inline scope as argument to code at block scope context
//!

use std::{cell::RefCell, rc::Rc};

use pyo3::{
    types::PyDict, IntoPy, Py, PyAny, PyClass, PyClassInitializer, PyObject, PyResult, Python,
};

use crate::{
    error::TurnipTextContextlessResult,
    interpreter::{
        eval_bracket::eval_or_exec, BlockScopeBuilder, InlineScopeBuilder, RawScopeBuilder,
    },
    lexer::{Escapable, LexError, TTToken},
    util::ParseSpan,
};

use super::{
    coerce_to_inline_pytcref, python::typeclass::PyTcRef, Block, BlockScope, BuilderOutcome,
    DocSegment, DocSegmentHeader, Inline, InlineScope, InterimDocumentStructure, InterpError,
    InterpreterFileAction, MapContextlessResult, Paragraph, Raw, Sentence, Text, TurnipTextSource,
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
    /// - newlines and scope-closes at the start of the line when in paragraph-mode - these scope closes are clearly intended for an enclosing block scope, so the paragraph should finish and the containing builder should handle the scope-close
    /// - EOFs in all non-error cases, because those should bubble up through the file
    /// - newlines at the end of comments, because those still signal the end of sentence in inline mode and count as a blank line in block mode
    /// - any token directly following an eval-bracket close that does not open a scope for the evaled code to own
    /// Scope-closes and raw-scope-closes crossing file boundaries invoke a (Raw)ScopeCloseOutsideScope error. EOFs and newlines are silently ignored.
    DoneAndReprocessToken(Option<PushToNextLevel>),
    Continue,
    StartInnerBuilder(Rc<RefCell<dyn BuildFromTokens>>),
    DoneAndNewSource(BuilderContext, TurnipTextSource),
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
    /// Note: this means if an impl doesn't override [BuildFromTokens::on_emitted_source_inside] to true then it will always receive tokens from the same file.
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
    // Make it opt-in to allow emitting new source files. By default, return an error. To opt-in, override to return Ok.
    fn on_emitted_source_inside(
        &self,
        from_builder: BuilderContext,
    ) -> TurnipTextContextlessResult<()> {
        Err(InterpError::InsertedFileMidPara {
            code_span: from_builder.from_span,
        }
        .into())
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
                    | BuildStatus::DoneAndNewSource(..) => {
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
                                // The token is bubbling up to the next file!
                                match tok {
                                    // ScopeClose bubbling out => breaking out into a subfile => not allowed
                                    TTToken::ScopeClose(span) => {
                                        return Err(InterpError::ScopeCloseOutsideScope(span).into())
                                    }
                                    // Ditto for RawScopeClose
                                    TTToken::RawScopeClose(span, _) => {
                                        return Err(
                                            InterpError::RawScopeCloseOutsideRawScope(span).into()
                                        )
                                    }
                                    // Don't let newlines inside a subfile affect outer files.
                                    TTToken::Newline(_) => return Ok(None),
                                    // EOF is handled in a separate place.
                                    // Other characters are passed through fine.
                                    _ => {}
                                }
                            } else {
                                // Don't return, keep going through the loop to bubble the token up to the next builder from this file
                            }
                        }
                        BuildStatus::DoneAndNewSource(from_builder, src) => {
                            self.stack.pop().expect("self.curr_top() returned Done => it must be processing a token from the file it was created in => it must be on the stack => the stack must have something on it.");
                            self.curr_top()
                                .borrow()
                                .on_emitted_source_inside(from_builder)?;
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
            BuildStatus::DoneAndReprocessToken(_) | BuildStatus::DoneAndNewSource(..) => unreachable!("process_push_from_inner_builder may not return DoneAndReprocessToken or DoneAndNewSource."),
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
        match tok {
            TTToken::Escaped(span, Escapable::Newline) => {
                Err(InterpError::EscapedNewlineOutsideParagraph { newline: span }.into())
            }
            TTToken::Whitespace(_) | TTToken::Newline(_) => Ok(BuildStatus::Continue),

            TTToken::Hashes(_, _) => Ok(BuildStatus::StartInnerBuilder(CommentFromTokens::new())),

            // Because this may return Inline we *always* have to be able to handle inlines at top scope.
            TTToken::CodeOpen(start_span, n_brackets) => Ok(BuildStatus::StartInnerBuilder(
                CodeFromTokens::new(start_span, n_brackets),
            )),

            TTToken::ScopeOpen(start_span) => Ok(BuildStatus::StartInnerBuilder(
                BlockOrInlineScopeFromTokens::new(start_span),
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

            TTToken::ScopeOpen(start_span) => {
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

            TTToken::CodeClose(span, _) => Err(InterpError::CodeCloseOutsideCode(span).into()),

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
}
impl TopLevelDocumentBuilder {
    fn new(py: Python) -> PyResult<Rc<RefCell<Self>>> {
        Ok(rc_refcell(Self {
            structure: InterimDocumentStructure::new(py)?,
        }))
    }

    fn finalize(mut self, py: Python) -> TurnipTextContextlessResult<Py<DocSegment>> {
        self.structure.pop_segments_until_less_than(py, i64::MIN)?;
        self.structure.finalize(py).err_as_internal(py)
    }
}
impl BlockTokenProcessor for TopLevelDocumentBuilder {
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
    // Don't error when someone tries to include a new file at the top level of a document
    fn on_emitted_source_inside(
        &self,
        from_builder: BuilderContext,
    ) -> TurnipTextContextlessResult<()> {
        Ok(())
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

/// This builder is initially started with a ScopeOpen token that may be a block scope open (followed by "\s*\n") or an inline scope open (followed by \s*[^\n]).
/// It starts out [BlockOrInlineScopeFromTokens::Undecided], then based on the following tokens either decides on [BlockOrInlineScopeFromTokens::Block] or [BlockOrInlineScopeFromTokens::Inline] and from then on acts as exactly [BlockScopeFromTokens] or [InlineScopeFromTokens] respectfully.
enum BlockOrInlineScopeFromTokens {
    Undecided { start_span: ParseSpan },
    Block(BlockScopeFromTokens),
    Inline(InlineScopeFromTokens),
}
impl BlockOrInlineScopeFromTokens {
    fn new(start_span: ParseSpan) -> Rc<RefCell<Self>> {
        rc_refcell(Self::Undecided { start_span })
    }
}
impl BuildFromTokens for BlockOrInlineScopeFromTokens {
    fn process_token(
        &mut self,
        py: Python,
        py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        match self {
            BlockOrInlineScopeFromTokens::Undecided { start_span } => match tok {
                TTToken::Whitespace(_) => Ok(BuildStatus::Continue),
                TTToken::EOF(_) => Err(InterpError::EndedInsideScope {
                    scope_start: *start_span,
                }
                .into()),
                TTToken::Newline(_) => {
                    // Transition to a block builder
                    let block_builder = BlockScopeFromTokens::new_unowned(py, *start_span)?;
                    // Block builder doesn't need to process the newline token specifically
                    // Swap ourselves out with the new state "i am a block builder"
                    let _ =
                        std::mem::replace(self, BlockOrInlineScopeFromTokens::Block(block_builder));
                    Ok(BuildStatus::Continue)
                }
                TTToken::Hashes(_, _) => {
                    Ok(BuildStatus::StartInnerBuilder(CommentFromTokens::new()))
                }
                _ => {
                    // Transition to an inline builder
                    let mut inline_builder = InlineScopeFromTokens::new_unowned(py, *start_span)?;
                    // Make sure it knows about the new token
                    let res = inline_builder.process_token(py, py_env, tok, data)?;
                    // Swap ourselves out with the new state "i am an inline builder"
                    let _ = std::mem::replace(
                        self,
                        BlockOrInlineScopeFromTokens::Inline(inline_builder),
                    );
                    Ok(res)
                }
            },
            BlockOrInlineScopeFromTokens::Block(block) => {
                block.process_token(py, py_env, tok, data)
            }
            BlockOrInlineScopeFromTokens::Inline(inline) => {
                inline.process_token(py, py_env, tok, data)
            }
        }
    }

    fn process_push_from_inner_builder(
        &mut self,
        py: Python,
        py_env: &PyDict,
        pushed: Option<PushToNextLevel>,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        match self {
            BlockOrInlineScopeFromTokens::Undecided { start_span: _ } => {
                assert!(pushed.is_none(), "BlockOrInlineScopeFromTokens::Undecided does not push any builders except comments thus cannot receive non-None pushed items");
                Ok(BuildStatus::Continue)
            }
            BlockOrInlineScopeFromTokens::Block(block) => {
                block.process_push_from_inner_builder(py, py_env, pushed)
            }
            BlockOrInlineScopeFromTokens::Inline(inline) => {
                inline.process_push_from_inner_builder(py, py_env, pushed)
            }
        }
    }

    fn on_emitted_source_inside(
        &self,
        from_builder: BuilderContext,
    ) -> TurnipTextContextlessResult<()> {
        match self {
            BlockOrInlineScopeFromTokens::Undecided { start_span: _ } => {
                unreachable!("BlockOrInlineScopeFromTokens::Undecided does not push any builders except comments and thus cannot have source code emitted inside it")
            }
            BlockOrInlineScopeFromTokens::Block(block) => {
                block.on_emitted_source_inside(from_builder)
            }
            BlockOrInlineScopeFromTokens::Inline(inline) => {
                inline.on_emitted_source_inside(from_builder)
            }
        }
    }
}

struct BlockScopeFromTokens {
    ctx: BuilderContext,
    block_scope: Py<BlockScope>,
}
impl BlockScopeFromTokens {
    fn new_unowned(py: Python, start_span: ParseSpan) -> TurnipTextContextlessResult<Self> {
        Ok(Self {
            ctx: BuilderContext::new("BlockScope", start_span),
            block_scope: py_internal_alloc(py, BlockScope::new_empty(py))?,
        })
    }
}
impl BlockTokenProcessor for BlockScopeFromTokens {
    fn on_close_scope(
        &mut self,
        py: Python,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        if !self.ctx.try_extend(&tok.token_span()) {
            // Closing block scope from different file
            Err(InterpError::ScopeCloseOutsideScope(tok.token_span()).into())
        } else {
            Ok(BuildStatus::Done(Some(self.ctx.make(DocElement::Block(
                PyTcRef::of_unchecked(self.block_scope.as_ref(py)),
            )))))
        }
    }

    fn on_eof(&mut self, py: Python, tok: TTToken) -> TurnipTextContextlessResult<BuildStatus> {
        Err(InterpError::EndedInsideScope {
            scope_start: self.ctx.from_span,
        }
        .into())
    }
}
impl BuildFromTokens for BlockScopeFromTokens {
    // Don't error when someone tries to include a new file inside a block scope
    fn on_emitted_source_inside(
        &self,
        from_builder: BuilderContext,
    ) -> TurnipTextContextlessResult<()> {
        Ok(())
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

struct ParagraphFromTokens {
    ctx: BuilderContext,
    para: Py<Paragraph>,
    start_of_line: bool,
    current_building_text: InlineTextState,
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
            current_building_text: InlineTextState::new(),
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
    fn at_start_of_line(&self) -> bool {
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
        // Text is already folded into the sentence
        if self.start_of_line {
            assert!(
                self.ctx.try_extend(&tok.token_span()),
                "ParagraphFromTokens got a token from a different file that it was opened in"
            );
            Ok(BuildStatus::DoneAndReprocessToken(Some(self.ctx.make(
                DocElement::Block(PyTcRef::of_unchecked(self.para.as_ref(py))),
            ))))
        } else {
            self.fold_current_sentence_into_paragraph(py)?;
            // We're now at the start of the line
            self.start_of_line = true;
            Ok(BuildStatus::Continue)
        }
    }

    fn on_eof(&mut self, py: Python, tok: TTToken) -> TurnipTextContextlessResult<BuildStatus> {
        if !self.start_of_line {
            self.fold_current_sentence_into_paragraph(py)?;
        }
        assert!(
            self.ctx.try_extend(&tok.token_span()),
            "ParagraphFromTokens got a token from a different file that it was opened in"
        );
        Ok(BuildStatus::DoneAndReprocessToken(Some(self.ctx.make(
            DocElement::Block(PyTcRef::of_unchecked(self.para.as_ref(py))),
        ))))
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
    start_of_line: bool,
    current_building_text: InlineTextState,
}
impl InlineScopeFromTokens {
    fn new(py: Python, start_span: ParseSpan) -> TurnipTextContextlessResult<Rc<RefCell<Self>>> {
        Ok(rc_refcell(Self::new_unowned(py, start_span)?))
    }

    fn new_unowned(py: Python, start_span: ParseSpan) -> TurnipTextContextlessResult<Self> {
        Ok(Self {
            ctx: BuilderContext::new("InlineScope", start_span),
            start_of_line: true,
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
impl InlineTokenProcessor for InlineScopeFromTokens {
    fn at_start_of_line(&self) -> bool {
        self.start_of_line
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
        assert!(
            self.ctx.try_extend(&tok.token_span()),
            "InlineScopeFromTokens got a token from a different file that it was opened in"
        );
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
    ctx: BuilderContext,
    n_closing: usize,
    code: String,
    evaled_code: Option<PyObject>,
}
impl CodeFromTokens {
    fn new(start_span: ParseSpan, n_opening: usize) -> Rc<RefCell<Self>> {
        rc_refcell(Self {
            ctx: BuilderContext::new("Code", start_span),
            n_closing: n_opening,
            code: String::new(),
            evaled_code: None,
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
        match &self.evaled_code {
            // If None, we're still parsing the code itself.
            None => {
                assert!(
                    self.ctx.try_extend(&tok.token_span()),
                    "Code got a token from a different file that it was opened in"
                );
                match tok {
                    TTToken::CodeClose(_, n_close_brackets)
                        if n_close_brackets == self.n_closing =>
                    {
                        let res: &PyAny = eval_or_exec(py, py_env, &self.code).err_as_interp(
                            py,
                            "Error evaluating contents of eval-brackets",
                            self.ctx.from_span,
                        )?;

                        // If we evaluated a TurnipTextSource, it may not be a builder of any kind thus we can finish immediately.
                        if let Ok(inserted_file) = res.extract::<TurnipTextSource>() {
                            Ok(BuildStatus::DoneAndNewSource(self.ctx, inserted_file))
                        } else {
                            self.evaled_code = Some(res.into_py(py));
                            Ok(BuildStatus::Continue)
                        }
                    }
                    TTToken::EOF(_) => Err(InterpError::EndedInsideCode {
                        code_start: self.ctx.from_span,
                    }
                    .into()),
                    _ => {
                        // Code blocks use raw stringification to avoid confusion between text written and text entered
                        self.code.push_str(tok.stringify_raw(data));
                        Ok(BuildStatus::Continue)
                    }
                }
            }
            // Parse one token after the code ends to see what we should do.
            Some(evaled_result) => match tok {
                TTToken::ScopeOpen(start_span) => Ok(BuildStatus::StartInnerBuilder(
                    BlockOrInlineScopeFromTokens::new(start_span),
                )),
                TTToken::RawScopeOpen(start_span, n_opening) => Ok(BuildStatus::StartInnerBuilder(
                    RawStringFromTokens::new(start_span, n_opening),
                )),

                _ => {
                    // We didn't encounter any scope openers, so we know we don't need to build anything.
                    // Emit the object directly, and reprocess the current token so it gets included.

                    // Consider: we may have an object at the very start of the line.
                    // If it's an Inline, e.g. "[virtio] is a thing..." then we want to return Inline so the rest of the line can be added.
                    // If it's a Block, e.g. [image_figure(...)], then we want to return Block.
                    // If it's neither, it needs to be *coerced*.
                    // But what should coercion look like? What should we try to coerce the object *to*?
                    // Well, what can be coerced?
                    // Coercible to inline:
                    // - Inline        -> `x`
                    // - List[Inline]  -> `InlineScope(x)`
                    // - str/float/int -> `Text(str(x))`
                    // Coercible to block:
                    // - Block             -> `x`
                    // - List[Block]       -> `BlockScope(x)`
                    // - Sentence          -> `Paragraph([x])
                    // - CoercibleToInline -> `Paragraph([Sentence([coerce_to_inline(x)])])`
                    // I do not see the need to allow eval-brackets to directly return List[Block] or Sentence at all.
                    // Similar outcomes can be acheived by wrapping in BlockScope or Paragraph manually in the evaluated code, which better demonstrates intent.
                    // If we always coerce to inline, then the wrapping in Paragraph and Sentence happens naturally in the interpreter.
                    // => We check if it's a block, and if it isn't we try to coerce to inline.

                    let evaled_result_ref = evaled_result.as_ref(py);

                    if evaled_result_ref.is_none() {
                        Ok(BuildStatus::DoneAndReprocessToken(None))
                    } else if let Ok(header) = PyTcRef::of(evaled_result_ref) {
                        Ok(BuildStatus::DoneAndReprocessToken(Some(
                            self.ctx.make(DocElement::Header(header)),
                        )))
                    } else if let Ok(block) = PyTcRef::of(evaled_result_ref) {
                        Ok(BuildStatus::DoneAndReprocessToken(Some(
                            self.ctx.make(DocElement::Block(block)),
                        )))
                    } else {
                        let inline = coerce_to_inline_pytcref(py, evaled_result_ref)
                            .err_as_interp(
                                py,
                                "Error while evaluating initial python code",
                                self.ctx.from_span,
                            )?;
                        Ok(BuildStatus::DoneAndReprocessToken(Some(
                            self.ctx.make(DocElement::Inline(inline)),
                        )))
                    }
                }
            },
        }
    }

    fn process_push_from_inner_builder(
        &mut self,
        py: Python,
        py_env: &PyDict,
        pushed: Option<PushToNextLevel>,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        let evaled_result_ref = self.evaled_code.take().unwrap().into_ref(py);

        let pushed = pushed.expect("Should never get a built None - CodeFromTokens only spawns BlockScopeFromTokens, InlineScopeFromTokens, RawScopeFromTokens none of which return None.");
        let built = match pushed.elem {
            DocElement::Block(blocks) => {
                let builder: PyTcRef<BlockScopeBuilder> =
                    PyTcRef::of_friendly(evaled_result_ref, "value returned by eval-bracket")
                    .err_as_interp(
                        py,
                        "Expected a BlockScopeBuilder because the eval-brackets were followed by a block scope", self.ctx.from_span
                    )?;

                // Now that we know coersion is a success, update the code span
                assert!(
                    self.ctx.try_extend(&pushed.from_builder.from_span),
                    "Code got a built object from a different file that it was opened in"
                );

                BlockScopeBuilder::call_build_from_blocks(py, builder, blocks.as_ref(py))
                    .err_as_internal(py)?
            }
            DocElement::Inline(inlines) => {
                let builder: PyTcRef<InlineScopeBuilder> =
                    PyTcRef::of_friendly(evaled_result_ref, "value returned by eval-bracket")
                    .err_as_interp(
                        py,
                        "Expected an InlineScopeBuilder because the eval-brackets were followed by an inline scope",
                        self.ctx.from_span
                    )?;

                // Now that we know coersion is a success, update the code span
                assert!(
                    self.ctx.try_extend(&pushed.from_builder.from_span),
                    "Code got a built object from a different file that it was opened in"
                );

                InlineScopeBuilder::call_build_from_inlines(py, builder, inlines.as_ref(py))
                    .err_as_internal(py)?
            }
            DocElement::Raw(raw) => {
                let builder: PyTcRef<RawScopeBuilder> =
                    PyTcRef::of_friendly(evaled_result_ref, "value returned by eval-bracket")
                    .err_as_interp(
                        py,
                        "Expected a RawScopeBuilder because the eval-brackets were followed by a raw scope",
                    self.ctx.from_span
                    )?;

                // Now that we know coersion is a success, update the code span
                assert!(
                    self.ctx.try_extend(&pushed.from_builder.from_span),
                    "Code got a built object from a different file that it was opened in"
                );

                RawScopeBuilder::call_build_from_raw(py, &builder, &raw).err_as_internal(py)?
            }
            _ => unreachable!("Invalid combination of requested and actual built element types"),
        };
        match built {
            BuilderOutcome::Block(block) => Ok(BuildStatus::Done(Some(
                self.ctx.make(DocElement::Block(block)),
            ))),
            BuilderOutcome::Inline(inline) => Ok(BuildStatus::Done(Some(
                self.ctx.make(DocElement::Inline(inline)),
            ))),
            BuilderOutcome::Header(header) => Ok(BuildStatus::Done(Some(
                self.ctx.make(DocElement::Header(header)),
            ))),
            BuilderOutcome::None => Ok(BuildStatus::Done(None)),
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
