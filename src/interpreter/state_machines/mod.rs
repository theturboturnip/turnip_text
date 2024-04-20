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
//! This builder structure introduces strict newline separation for blocks:
//! - after eval-bracket-emitting-block-or-header-or-inserted-file-or-none, a newline must be seen before any content
//! - after closing a block-level block scope (i.e. ignoring block scopes used as arguments to code producing inline), a newline must be seen before any content
//! There's no need to worry about this with paragraphs (double newline required to end the paragraph) and block scope opens/closes (extra newlines between opening new block scopes seems superfluous, opening a block scope requires a newline, block scopes can't be closed mid paragraph)
//! This means newlines must bubble up through files (blocks inside an inner file are governed by either the top-level document token builder or an enclosing block scope builder, both of which are "the next file up"), and I previously worried that this would mean newlines in an inner file would affect an outer file.
//! Not so!
//! Note that the first condition requires that after eval-bracket-emitting-inserted-file, a newline must be seen before any content.
//! The newline-consuming state of the outer builder is always reset when an inserted file is opened and after an inserted file is closed, so inserted files can't have an inconsistent impact on enclosing files and vice versa.
//!
//! TODO update documentation
//! TODO rename stuff to be better and consistent

use std::{cell::RefCell, rc::Rc};

use pyo3::{prelude::*, types::PyDict, PyClass};

use crate::{
    error::{
        interp::{InterpError, MapContextlessResult},
        TurnipTextContextlessResult,
    },
    lexer::TTToken,
    python::{
        interop::{
            Block, BlockScope, DocSegmentHeader, Inline, InlineScope, Paragraph, Raw,
            TurnipTextSource,
        },
        typeclass::PyTcRef,
    },
    util::ParseSpan,
};

mod block;
pub use block::TopLevelDocumentBuilder;

mod code;
mod comment;
mod inline;

/// An enum encompassing all the things that can be directly emitted from one Builder to be bubbled up to the previous Builder.
///
/// Doesn't include TurnipTextSource - that is emitted from Python but it needs to bypass everything and go to the top-level interpreter
enum DocElement {
    Block(BlockElem),
    Inline(InlineElem),
    HeaderFromCode(PyTcRef<DocSegmentHeader>),
}

enum BlockElem {
    FromCode(PyTcRef<Block>),
    BlockScope(Py<BlockScope>),
    Para(Py<Paragraph>),
}
impl BlockElem {
    fn as_ref<'py>(&'py self, py: Python<'py>) -> &'py PyAny {
        match self {
            BlockElem::FromCode(b) => b.as_ref(py),
            BlockElem::BlockScope(bs) => bs.as_ref(py),
            BlockElem::Para(p) => p.as_ref(py),
        }
    }
}
impl From<BlockElem> for DocElement {
    fn from(value: BlockElem) -> Self {
        Self::Block(value)
    }
}

enum InlineElem {
    FromCode(PyTcRef<Inline>),
    InlineScope(Py<InlineScope>),
    Raw(Py<Raw>),
}
impl InlineElem {
    fn as_ref<'py>(&'py self, py: Python<'py>) -> &'py PyAny {
        match self {
            InlineElem::FromCode(i) => i.as_ref(py),
            InlineElem::InlineScope(is) => is.as_ref(py),
            InlineElem::Raw(r) => r.as_ref(py),
        }
    }
}
impl From<InlineElem> for DocElement {
    fn from(value: InlineElem) -> Self {
        Self::Inline(value)
    }
}

struct PushToNextLevel {
    from_builder: ParseContext,
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
    /// Scope-closes and raw-scope-closes crossing file boundaries invoke a (Raw)ScopeCloseOutsideScope error. Newlines are passed through. EOFs are silently ignored.
    DoneAndReprocessToken(Option<PushToNextLevel>),
    Continue,
    StartInnerBuilder(Rc<RefCell<dyn BuildFromTokens>>),
    DoneAndNewSource(ParseContext, TurnipTextSource),
}

#[derive(Debug, Clone, Copy)]
pub struct ParseContext {
    first_tok: ParseSpan,
    last_tok: ParseSpan,
}
impl ParseContext {
    fn new(first_tok: ParseSpan, last_tok: ParseSpan) -> Self {
        assert!(
            first_tok.file_idx() == last_tok.file_idx(),
            "Can't have a BuilderContext span two files"
        );
        Self {
            first_tok,
            last_tok,
        }
    }
    fn try_extend(&mut self, new_tok: &ParseSpan) -> bool {
        if new_tok.file_idx() == self.last_tok.file_idx() {
            assert!(self.first_tok.start().byte_ofs <= new_tok.start().byte_ofs);
            self.last_tok = *new_tok;
            true
        } else {
            false
        }
    }
    fn try_combine(&mut self, new_builder: ParseContext) -> bool {
        if new_builder.first_tok.file_idx() == self.first_tok.file_idx() {
            assert!(self.first_tok.start().byte_ofs <= new_builder.first_tok.start().byte_ofs);
            self.last_tok = new_builder.last_tok;
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

    pub fn first_tok(&self) -> ParseSpan {
        self.first_tok
    }
    pub fn last_tok(&self) -> ParseSpan {
        self.last_tok
    }
    pub fn full_span(&self) -> ParseSpan {
        self.first_tok.combine(&self.last_tok)
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
        &mut self,
        from_builder: ParseContext,
    ) -> TurnipTextContextlessResult<()> {
        Err(InterpError::InsertedFileMidPara {
            code_span: from_builder.full_span(),
        }
        .into())
    }
    // Called when an inner file is closed. Will only be called if on_emitted_source_inside() is overridden to sometimes return Ok().
    fn on_emitted_source_closed(&mut self, _inner_source_emitted_by: ParseSpan) {}
}

pub struct FileBuilderStack {
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

    pub fn process_token(
        &mut self,
        py: Python,
        py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<Option<(ParseContext, TurnipTextSource)>> {
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
                                    // ScopeClose bubbling out => breaking out into a subfile => not allowed.
                                    // This must be a block-level scope close, because if an unbalanced scope close appeared in inline mode it would already have errored and not bubbled out.
                                    TTToken::ScopeClose(span) => {
                                        return Err(
                                            InterpError::BlockScopeCloseOutsideScope(span).into()
                                        )
                                    }
                                    // Ditto for RawScopeClose
                                    TTToken::RawScopeClose(span, _) => {
                                        return Err(
                                            InterpError::RawScopeCloseOutsideRawScope(span).into()
                                        )
                                    }
                                    // Let newlines inside a subfile affect outer files, and trust that those outer builders will reset their state correctly when this file ends.
                                    TTToken::Newline(_) => {}
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
                                .borrow_mut()
                                .on_emitted_source_inside(from_builder)?;
                            return Ok(Some((from_builder, src)));
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
pub struct BuilderStacks {
    top: Rc<RefCell<TopLevelDocumentBuilder>>,
    /// The stacks of builders, one stack per file.
    /// If empty, the file is about to be finalized and functions for getting/popping a stack must not be called.
    builder_stacks: Vec<FileBuilderStack>,
}
impl BuilderStacks {
    pub fn new(py: Python) -> PyResult<Self> {
        let top = TopLevelDocumentBuilder::new(py)?;
        Ok(Self {
            builder_stacks: vec![FileBuilderStack::new(top.clone())], // Constant condition: there is always at least one builder stack
            top,
        })
    }

    pub fn top_stack(&mut self) -> &mut FileBuilderStack {
        self.builder_stacks
            .last_mut()
            .expect("Must always have at least one builder stack")
    }

    pub fn push_subfile(&mut self) {
        let topmost_builder_enclosing_new_file = self.top_stack().curr_top().clone();
        self.builder_stacks
            .push(FileBuilderStack::new(topmost_builder_enclosing_new_file))
    }
    pub fn pop_subfile(&mut self, subfile_emitted_by: Option<ParseSpan>) {
        let stack = self
            .builder_stacks
            .pop()
            .expect("Must always have at least one builder stack");
        assert!(stack.stack.is_empty());
        // emitted_by will be Some if the file was a subfile
        if let Some(subfile_emitted_by) = subfile_emitted_by {
            // Notify the next builder up that we've re-entered it after the inner file closed
            self.top_stack()
                .curr_top()
                .borrow_mut()
                .on_emitted_source_closed(subfile_emitted_by);
        }
    }

    pub fn finalize(self) -> Rc<RefCell<TopLevelDocumentBuilder>> {
        if self.builder_stacks.len() > 0 {
            panic!("Called finalize() on BuilderStacks when more than one stack was left - forgot to pop_subfile()?");
        }
        self.top
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
