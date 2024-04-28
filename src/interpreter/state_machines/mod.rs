//! The combinatorial explosion of moving between block/inline states hsa gotten too much to handle.
//! It's also inconvenient - e.g. `[thing]{contents}` may only emit an Inline, even if it's the only thing on the line and looks like it could emit Block, because the parser moves to "inline mode" and can't handle getting a Block out of that.
//! The correct course of action would be for the code processor to compute the inner inline, pass it into the processor, realize it got a Block out, and emit that block directly.
//! I'm envisioning a system with a stack of processors: every time a new token is received it's given to the topmost processor on the stack, which can return
//! - an error, stopping everything
//! - a completed object, implicitly popping the stack of processors
//! - a new processor to push to the stack
//!
//! If a completed object is returned, the processor is popped and the object is passed into the next processor on the stack to be integrated into the contents.
//! This method is convenient because it handles other alt-states for parsing such as comments and raw strings naturally by putting them on top of the stack!
//!
//! Each file has a separate [FileProcessorstack], which falls back to the topmost processor in the previous file or the topmost processor of the whole document if no containing files have processors.
//! It's possible to have a tall stack of files where no file is using a processor - e.g. if you have file A include
//! file B include file C include file D, and you just write paragraphs, files A through C will have empty processor stacks while file D is being processed and paragraphs from file D will bubble right up to the top-level document.
//!
//! This processor structure introduces strict newline separation for blocks:
//! - after eval-bracket-emitting-block-or-header-or-inserted-file-or-none, a blank line must be seen before any content
//! - after closing a block-level block scope (i.e. ignoring block scopes used as arguments to code producing inline), a blank line must be seen before any content
//! There's no need to worry about this with paragraphs (double newline required to end the paragraph) and block scope opens/closes (extra newlines between opening new block scopes seems superfluous, opening a block scope requires a newline, block scopes can't be closed mid paragraph)
//! This means newlines must bubble up through files (blocks inside an inner file are governed by either the top-level document token processor or an enclosing block scope processor, both of which are "the next file up"), and I previously worried that this would mean newlines in an inner file would affect an outer file.
//! Not so!
//! Note that the first condition requires that after eval-bracket-emitting-inserted-file, a newline must be seen before any content.
//! The newline-consuming state of the outer processor is always reset when an inserted file is opened and after an inserted file is closed, so inserted files can't have an inconsistent impact on enclosing files and vice versa.
//!
//! TODO update documentation

use std::{cell::RefCell, rc::Rc};

use pyo3::{prelude::*, types::PyDict, PyClass};

use crate::{
    error::{
        interp::{BlockModeElem, InterpError, MapContextlessResult},
        TurnipTextContextlessResult,
    },
    lexer::TTToken,
    python::{
        interop::{
            Block, BlockScope, DocSegment, DocSegmentHeader, Inline, InlineScope, Paragraph, Raw,
            TurnipTextSource,
        },
        typeclass::PyTcRef,
    },
    util::{ParseContext, ParseSpan},
};

mod ambiguous_scope;
mod block;
use block::TopLevelProcessor;

use super::FileEvent;

mod code;
mod comment;
mod inline;

/// An enum encompassing all the things that can be directly emitted from one Processor to be bubbled up to the previous Processor.
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
impl From<(ParseContext, &BlockElem)> for BlockModeElem {
    fn from(value: (ParseContext, &BlockElem)) -> Self {
        match value.1 {
            BlockElem::FromCode(_) => BlockModeElem::BlockFromCode(value.0.full_span()),
            BlockElem::BlockScope(_) => BlockModeElem::BlockScope(value.0),
            BlockElem::Para(_) => BlockModeElem::Para(value.0),
        }
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

type EmittedElement = (ParseContext, DocElement);

trait TokenProcessor {
    /// This will usually receive tokens from the same file it was created in, unless a source file is opened within it
    /// in which case it will receive the top-level tokens from that file too except EOF.
    ///
    /// Note: this means if an impl doesn't override [TokenProcessor::on_emitted_source_inside] to true then it will always receive tokens from the same file.
    ///
    /// When receiving any token from an inner file, this function must return either an error, [ProcStatus::Continue], or [ProcStatus::PushProcessor]. Other responses would result in modifying the outer file due to the contents of the inner file, and are not allowed.
    ///
    /// When receiving [TTToken::EOF] this function must return either an error or [ProcStatus::PopAndReprocessToken]. Other responses are not allowed.
    fn process_token(
        &mut self,
        py: Python,
        py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<ProcStatus>;
    /// Handle an emitted element from a processor you pushed.
    /// May only return an error, [ProcStatus::Continue], [ProcStatus::PushProcessor], or [ProcStatus::Pop].
    fn process_emitted_element(
        &mut self,
        py: Python,
        py_env: &PyDict,
        emitted: Option<EmittedElement>,
    ) -> TurnipTextContextlessResult<ProcStatus>;
    /// Called when a processor you pushed returns [ProcStatus::PopAndNewSource].
    fn on_emitted_source_inside(
        &mut self,
        code_emitting_source: ParseContext,
    ) -> TurnipTextContextlessResult<()>;
    /// Called when a source file emitted by a processor you pushed is closed.
    /// Will not be called if [on_emitted_source_inside] returned an error.
    fn on_emitted_source_closed(&mut self, inner_source_emitted_by: ParseSpan);
}

enum ProcStatus {
    /// Keep processing tokens, don't modify the stacks.
    Continue,
    /// Push a new processor onto the stack, which will consume the following tokens instead of you
    /// until it Pops itself.
    PushProcessor(Rc<RefCell<dyn TokenProcessor>>),
    /// Pop the current processor off, and optionally emit an element into the processor that spawned you.
    Pop(Option<EmittedElement>),
    /// Pop the current processor off, optionally emit an element into the processor that spawned you,
    /// and make that outer processor reprocess the current token.
    ///
    /// On rare occasions it is necessary to bubble the token up to the next processor as well as the finished item.
    /// This applies to
    /// - newlines and scope-closes at the start of the line when in paragraph-mode - these scope closes are clearly intended for an enclosing block scope, so the paragraph should finish and the containing processor should handle the scope-close
    /// - EOFs in all non-error cases, because those should bubble up through the file
    /// - newlines at the end of comments, because those still signal the end of sentence in inline mode and count as a blank line in block mode
    /// - any token directly following an eval-bracket close that does not open a scope for the evaled code to own
    /// Scope-closes and raw-scope-closes crossing file boundaries invoke a (Raw)ScopeCloseOutsideScope error. Newlines are passed through. EOFs are silently ignored.
    PopAndReprocessToken(Option<EmittedElement>),
    /// Pop the current processor off, emitting a new source file that is pushed to the top of the file stack.
    /// Its tokens will be consumed next until it ends or a new source is pushed.
    PopAndNewSource(ParseContext, TurnipTextSource),
}

/// Holds the stack of processors for a single file,
/// and a reference to the TokenProcessor that emitted the subfile.
pub struct FileProcessorStack {
    top: Rc<RefCell<dyn TokenProcessor>>,
    /// The stack of processors created inside this file.
    stack: Vec<Rc<RefCell<dyn TokenProcessor>>>,
}
impl FileProcessorStack {
    fn new(top: Rc<RefCell<dyn TokenProcessor>>) -> Self {
        Self { top, stack: vec![] }
    }

    fn curr_top(&self) -> &Rc<RefCell<dyn TokenProcessor>> {
        match self.stack.last() {
            None => &self.top,
            Some(top) => top,
        }
    }

    pub fn process_tokens(
        &mut self,
        py: Python,
        py_env: &PyDict,
        toks: &mut impl Iterator<Item = TTToken>,
        data: &str,
    ) -> TurnipTextContextlessResult<FileEvent> {
        for tok in toks {
            match self.process_token(py, py_env, tok, data)? {
                None => continue,
                Some((emitted_by_code, TurnipTextSource { name, contents })) => {
                    return Ok(FileEvent::FileInserted {
                        emitted_by_code,
                        name,
                        contents,
                    });
                }
            }
        }
        // We have exhausted the token stream
        Ok(FileEvent::FileEnded)
    }

    fn process_token(
        &mut self,
        py: Python,
        py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<Option<(ParseSpan, TurnipTextSource)>> {
        // If processing an EOF we need to flush all processors in the stack and not pass through tokens to self.top()
        if let TTToken::EOF(_) = &tok {
            loop {
                let top = match self.stack.pop() {
                    None => break,
                    Some(top) => top,
                };
                let action = top.borrow_mut().process_token(py, py_env, tok, data)?;
                match action {
                    ProcStatus::PopAndReprocessToken(emitted) => {
                        self.emit_elem_in_top_processor(py, py_env, emitted)?
                    }
                    _ => unreachable!(
                        "processor returned a ProcStatus that wasn't PopAndReprocessToken in \
                         response to an EOF"
                    ),
                }
            }
            Ok(None)
        } else {
            // The token is not EOF, so we are allowed to pass the token through to self.top.
            // If there are no processors, we can pass the token to the top processor and see what it says.
            if self.stack.is_empty() {
                let action = self.top.borrow_mut().process_token(py, py_env, tok, data)?;
                match action {
                    ProcStatus::Pop(_)
                    | ProcStatus::PopAndReprocessToken(_)
                    | ProcStatus::PopAndNewSource(..) => {
                        unreachable!(
                            "processor for previous file returned a Pop* when presented with a \
                             token for an inner file"
                        )
                    }
                    ProcStatus::PushProcessor(processor) => self.stack.push(processor),
                    ProcStatus::Continue => {}
                };
                Ok(None)
            } else {
                // If there are processors, pass the token to the topmost one.
                // The token may bubble out if the processor returns PopAndReprocessToken, so loop to support that case and break out with returns otherwise.
                loop {
                    let action = self
                        .curr_top()
                        .borrow_mut()
                        .process_token(py, py_env, tok, data)?;
                    match action {
                        ProcStatus::Continue => return Ok(None),
                        ProcStatus::PushProcessor(processor) => {
                            self.stack.push(processor);
                            return Ok(None);
                        }
                        ProcStatus::Pop(emitted) => {
                            self.stack.pop().expect(
                                "self.curr_top() returned Pop => it must be processing a token \
                                 from the file it was created in => it must be on the stack => \
                                 the stack must have something on it.",
                            );
                            self.emit_elem_in_top_processor(py, py_env, emitted)?;
                            return Ok(None);
                        }
                        ProcStatus::PopAndReprocessToken(emitted) => {
                            self.stack.pop().expect(
                                "self.curr_top() returned Pop => it must be processing a token \
                                 from the file it was created in => it must be on the stack => \
                                 the stack must have something on it.",
                            );
                            self.emit_elem_in_top_processor(py, py_env, emitted)?;
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
                                    // Let newlines inside a subfile affect outer files, and trust that those outer processors will reset their state correctly when this file ends.
                                    TTToken::Newline(_) => {}
                                    // EOF is handled in a separate place.
                                    // Other characters are passed through fine.
                                    _ => {}
                                }
                            } else {
                                // Don't return, keep going through the loop to bubble the token up to the next processor from this file
                            }
                        }
                        ProcStatus::PopAndNewSource(code_emitting_source, src) => {
                            self.stack.pop().expect(
                                "self.curr_top() returned Pop => it must be processing a token \
                                 from the file it was created in => it must be on the stack => \
                                 the stack must have something on it.",
                            );
                            self.curr_top()
                                .borrow_mut()
                                .on_emitted_source_inside(code_emitting_source)?;
                            return Ok(Some((code_emitting_source.full_span(), src)));
                        }
                    }
                }
            }
        }
    }

    fn emit_elem_in_top_processor(
        &mut self,
        py: Python,
        py_env: &PyDict,
        emitted: Option<EmittedElement>,
    ) -> TurnipTextContextlessResult<()> {
        let action = self
            .curr_top()
            .borrow_mut()
            .process_emitted_element(py, py_env, emitted)?;
        match action {
            ProcStatus::Continue => Ok(()),
            ProcStatus::Pop(new_emitted) => {
                self.stack.pop().expect(
                    "self.curr_top() returned Pop => it must be processing a token from the file \
                     it was created in => it must be on the stack => the stack must have \
                     something on it.",
                );
                self.emit_elem_in_top_processor(py, py_env, new_emitted)
            }
            ProcStatus::PushProcessor(processor) => {
                self.stack.push(processor);
                Ok(())
            }
            ProcStatus::PopAndReprocessToken(_) | ProcStatus::PopAndNewSource(..) => unreachable!(
                "process_emitted_element may not return PopAndReprocessToken or PopAndNewSource."
            ),
        }
    }
}

/// Holds multiple stacks of processors including an always-present top level processor.
/// Each stack of processors
pub struct ProcessorStacks {
    top: Rc<RefCell<TopLevelProcessor>>,
    /// The stacks of processors, one stack per file.
    /// If empty, the file is about to be finalized and functions for getting/popping a stack must not be called.
    stacks: Vec<FileProcessorStack>,
}
impl ProcessorStacks {
    pub fn new(py: Python) -> PyResult<Self> {
        let top = rc_refcell(TopLevelProcessor::new(py)?);
        Ok(Self {
            stacks: vec![FileProcessorStack::new(top.clone())], // Constant condition: there is always at least one processor stack
            top,
        })
    }

    pub fn top_stack(&mut self) -> &mut FileProcessorStack {
        self.stacks
            .last_mut()
            .expect("Must always have at least one processor stack")
    }

    pub fn push_subfile(&mut self) {
        let topmost_processor_enclosing_new_file = self.top_stack().curr_top().clone();
        self.stacks.push(FileProcessorStack::new(
            topmost_processor_enclosing_new_file,
        ))
    }
    pub fn pop_subfile(&mut self, subfile_emitted_by: Option<ParseSpan>) {
        let stack = self
            .stacks
            .pop()
            .expect("Must always have at least one processor stack");
        assert!(stack.stack.is_empty());
        // emitted_by will be Some if the file was a subfile
        if let Some(subfile_emitted_by) = subfile_emitted_by {
            // Notify the next processor up that we've re-entered it after the inner file closed
            self.top_stack()
                .curr_top()
                .borrow_mut()
                .on_emitted_source_closed(subfile_emitted_by);
        }
    }

    pub fn finalize(self, py: Python) -> TurnipTextContextlessResult<Py<DocSegment>> {
        if self.stacks.len() > 0 {
            panic!(
                "Called finalize() on Processorstacks when there were stacks left - forgot to \
                 pop_subfile()?"
            );
        }
        match Rc::try_unwrap(self.top) {
            Err(_) => panic!(),
            Ok(refcell_top) => refcell_top.into_inner().finalize(py),
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
