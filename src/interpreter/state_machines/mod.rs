//! The interpreter is structured in stacks of separate [TokenProcessor]s.
//! The interpreter may be handling multiple files at a time  - e.g. if code inside [TurnipTextSource] `A` emits a [TurnipTextSource] `B`, the tokens from `B` must be processed before we can continue in `A`.
//! In this case `B` is the "enclosed" or "inner" file and `A` is the "enclosing" or "surrounding" or "outer" file.
//! Each file has a separate [FileProcessorStack] of [TokenProcessor]s, which is initially empty.
//! [ProcessorStacks] holds all the [FileProcessorStack]s of currently-processing files.
//!
//! When a new token arrives, it is processed by either
//! - the last [TokenProcessor] on the file's stack, if the stack is not empty
//! - otherwise the topmost [TokenProcessor] for the surrounding file, if there is one
//!     - as it happens, this must be a [block::BlockScopeProcessor]
//! - otherwise the very outer [TopLevelProcessor] which is responsible for constructing the final [Document]
//!
//! Processing a token can have five results, enumerated in [ProcStatus].
//! - If the processor is still consuming tokens, [ProcStatus::Continue]
//! - If the processor encounters a token that requires a state change (e.g. a new open scope token), [ProcStatus::PushProcessor]
//!     - This pushes a new [TokenProcessor] onto the current file's stack
//! - If the processor has finished (e.g. a scope processor has consumed a closing scope token), [ProcStatus::Pop]
//!     - This pops the current [TokenProcessor] from the current file's stack, and must only be returned when processing a token from the same file as the [TokenProcessor]
//!     - It also *emits* a [DocElement] into the next [TokenProcessor] up. For example, if a [block::BlockScopeProcessor] encounters a content token, it creates a [inline::ParagraphProcessor], and that eventually emits a [BlockElem::Para] for the [block::BlockScopeProcessor] to handle.
//! - If the processor has finished, but the outer processor is interested in the token, the inner processor returns [ProcStatus::PopAndReprocessToken]
//!     - This does the same pop and emit, but the next available processor is also given the token and could return [ProcStatus::PopAndReprocessToken] as well!
//!     - This is used for e.g. comments, which are ended by newlines, escaped newlines, and EOF tokens - all of which are relevant to the next levels up, because the syntax is newline-sensitive
//! - If the processor has finished, and it wants to emit a new [TurnipTextSource] (i.e. this processor is a [code::CodeProcessor] that evaluated user-python and got a new source), [ProcStatus::PopAndNewSource].
//!
//! When a processor returns [ProcStatus::PopAndNewSource], the [FileProcessorStack] calls [TokenProcessor::on_emitted_source_inside] on the enclosing processor.
//! In some cases, such as for paragraphs and inline scopes, the enclosing processor will return an [Err], blocking the new source because they aren't allowed in those contexts.
//! If a processor returns [Err] here, it will never receive tokens from other files.
//! If a processor returns [Ok] here, it will be the "topmost TokenProcessor for the surrounding file" and thus receive tokens from enclosed files.
//!
//! There are specific requirements and restrictions as to which [ProcStatus] values can be returned when - see [ProcStatus] and [TokenProcessor] documentation for more information.
//!
//! ## Context
//! Previously there was a hardcoded two-level state machine: one for block mode, and one for inline mode,
//! which only allowed block scopes in block mode (even if they were attached to arbitrary builders!) and inline scopes in inline mode.
//! This was problematic as e.g. you couldn't build a [Header] from an [InlineScope] -
//! going into inline mode at any point would put the whole parser into inline mode,
//! and the [Header] would be rejected (not allowed in inline mode!) once emitted.
//! This system is more flexible, and elegantly handles the special-case behaviours for raw scopes, eval brackets,
//! and comments.

use std::{cell::RefCell, rc::Rc};

use pyo3::{prelude::*, PyClass};

use crate::{
    error::{
        syntax::{BlockModeElem, TTSyntaxError},
        TTResult,
    },
    lexer::TTToken,
    python::{
        interop::{
            Block, BlockScope, Document, Header, Inline, InlineScope, Paragraph, Raw,
            TurnipTextSource,
        },
        typeclass::PyTcRef,
    },
    util::{ParseContext, ParseSpan},
};

mod ambiguous_scope;
mod block;
use block::TopLevelProcessor;

use super::{FileEvent, ParserEnv};

mod code;
mod comment;
mod inline;

/// An enum encompassing all the things that can be directly emitted from one Processor to be bubbled up to the previous Processor.
///
/// Doesn't include TurnipTextSource - that is emitted from Python but it needs to bypass everything and go to the top-level interpreter
#[derive(Debug)]
enum DocElement {
    Block(BlockElem),
    Inline(InlineElem),
    HeaderFromCode(PyTcRef<Header>),
}

#[derive(Debug)]
enum BlockElem {
    FromCode(PyTcRef<Block>),
    BlockScope(Py<BlockScope>),
    Para(Py<Paragraph>),
}
impl BlockElem {
    fn bind<'py>(&'py self, py: Python<'py>) -> &Bound<'py, PyAny> {
        match self {
            BlockElem::FromCode(b) => b.bind(py),
            BlockElem::BlockScope(bs) => bs.bind(py),
            BlockElem::Para(p) => p.bind(py),
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

#[derive(Debug)]
enum InlineElem {
    FromCode(PyTcRef<Inline>),
    InlineScope(Py<InlineScope>),
    Raw(Py<Raw>),
}
impl InlineElem {
    fn bind<'py>(&'py self, py: Python<'py>) -> &Bound<'py, PyAny> {
        match self {
            InlineElem::FromCode(i) => i.bind(py),
            InlineElem::InlineScope(is) => is.bind(py),
            InlineElem::Raw(r) => r.bind(py),
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
        py_env: ParserEnv,
        tok: TTToken,
        data: &str,
    ) -> TTResult<ProcStatus>;
    /// Handle an emitted element from a processor you pushed.
    /// May only return an error, [ProcStatus::Continue], [ProcStatus::PushProcessor], or [ProcStatus::Pop].
    fn process_emitted_element(
        &mut self,
        py: Python,
        py_env: ParserEnv,
        emitted: Option<EmittedElement>,
    ) -> TTResult<ProcStatus>;
    /// Called when a processor you pushed returns [ProcStatus::PopAndNewSource].
    fn on_emitted_source_inside(&mut self, code_emitting_source: ParseContext) -> TTResult<()>;
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
        py_env: ParserEnv,
        toks: &mut impl Iterator<Item = TTToken>,
        data: &str,
    ) -> TTResult<FileEvent> {
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
        py_env: ParserEnv,
        tok: TTToken,
        data: &str,
    ) -> TTResult<Option<(ParseSpan, TurnipTextSource)>> {
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
                                        return Err(TTSyntaxError::BlockScopeCloseOutsideScope(
                                            span,
                                        )
                                        .into())
                                    }
                                    // Ditto for RawScopeClose
                                    TTToken::RawScopeClose(span, _) => {
                                        return Err(TTSyntaxError::RawScopeCloseOutsideRawScope(
                                            span,
                                        )
                                        .into())
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
        py_env: ParserEnv,
        emitted: Option<EmittedElement>,
    ) -> TTResult<()> {
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

    pub fn finalize(self, py: Python) -> TTResult<Py<Document>> {
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
) -> TTResult<Py<T>> {
    Ok(Py::new(py, value)?)
}

fn rc_refcell<T>(t: T) -> Rc<RefCell<T>> {
    Rc::new(RefCell::new(t))
}
