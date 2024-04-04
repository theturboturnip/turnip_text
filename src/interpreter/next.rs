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
//! TODO this method currently doesn't split up BlockScopes by file - i.e. if you open a scope in one file, go into a subfile, then close it inside the subfile, that's allowed. Need to add a check inside BlockScopeBuilder(?) that the closing scope is in the same file as the opening scope

use pyo3::{types::PyDict, Py, PyAny, PyClass, PyClassInitializer, PyResult, Python};

use crate::{
    error::TurnipTextContextlessResult,
    lexer::{LexError, TTToken},
    util::ParseSpan,
};

use super::{
    eval_bracket::{eval_brackets, EvalBracketResult},
    python::typeclass::PyTcRef,
    Block, BlockScope, DocSegment, DocSegmentHeader, Inline, InterimDocumentStructure,
    InterpreterFileAction, MapContextlessResult, Raw, TurnipTextSource,
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
    ) -> TurnipTextContextlessResult<BuildStatus>;
    // Some builders may produce something on EOF e.g. a Paragraph builder will just return the Paragraph up to this point
    fn process_eof(
        &mut self,
        py: Python,
        py_env: &PyDict,
    ) -> TurnipTextContextlessResult<Option<PushToNextLevel>>;
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
        self.handle_build_status(py, py_env, status)
    }

    fn handle_build_status(
        &mut self,
        py: Python,
        py_env: &PyDict,
        status: BuildStatus,
    ) -> TurnipTextContextlessResult<Option<TurnipTextSource>> {
        match status {
            BuildStatus::Done(pushed) => {
                self.builders.pop_from_current_file().expect("We just parsed something using the top of self.builders, it must be able to pop");
                let next_status = self
                    .builders
                    .top_builder()
                    .process_push_from_inner_builder(py, py_env, pushed)?;
                return self.handle_build_status(py, py_env, status);
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
    ) -> TurnipTextContextlessResult<()> {
        let mut stack = self.builders.pop_subfile();
        // If there are any builders within the stack, tell them about the EOF and bubble their production up to the next level.
        match stack.pop() {
            Some(mut builder) => {
                let mut pushed = builder.process_eof(py, py_env)?;
                while let Some(mut builder) = stack.pop() {
                    pushed = {
                        builder.process_push_from_inner_builder(py, py_env, pushed)?;
                        builder.process_eof(py, py_env)?
                    };
                }
                // If there were builders, then a new element (which may be None!) was produced and we need to bubble it up to the next file
                self.builders
                    .top_builder()
                    .process_push_from_inner_builder(py, py_env, pushed)?;
            }
            // If there weren't any builders, we don't need to do anything
            None => {}
        };
        Ok(())
    }

    pub fn finalize<'a>(
        mut self,
        py: Python,
        py_env: &PyDict,
    ) -> TurnipTextContextlessResult<Py<DocSegment>> {
        let (mut top, mut stack) = self.builders.finalize();

        // If there are any builders within the stack, tell them about the EOF and bubble their production up to the next level.
        match stack.pop() {
            Some(mut builder) => {
                let mut pushed = builder.process_eof(py, py_env)?;
                while let Some(mut builder) = stack.pop() {
                    pushed = {
                        builder.process_push_from_inner_builder(py, py_env, pushed)?;
                        builder.process_eof(py, py_env)?
                    };
                }
                // If there were builders, then a new_elem was produced and we need to bubble it up to the next file
                top.process_push_from_inner_builder(py, py_env, pushed)?;
            }
            // If there weren't any builders, we don't need to do anything
            None => {}
        };

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

fn process_block_level_token<F>(
    py: Python,
    py_env: &PyDict,
    tok: TTToken,
    data: &str,
    expect_newline: &mut bool,
    on_close_scope: F,
) -> TurnipTextContextlessResult<BuildStatus>
where
    F: FnOnce(Python, &PyDict, TTToken, &str) -> TurnipTextContextlessResult<BuildStatus>,
{
    if *expect_newline {
        match tok {
            TTToken::Whitespace(_) => Ok(BuildStatus::Continue),
            TTToken::Newline(_) => {
                *expect_newline = false;
                Ok(BuildStatus::Continue)
            }

            _ => todo!("Error"),
        }
    } else {
        match tok {
            TTToken::Whitespace(_) | TTToken::Newline(_) => Ok(BuildStatus::Continue),

            TTToken::Hashes(_, _) => Ok(BuildStatus::StartInnerBuilder(CommentFromTokens::new())),

            // Because this may return Inline we *always* have to be able to handle inlines at top scope.
            TTToken::CodeOpen(start_span, n_brackets) => Ok(BuildStatus::StartInnerBuilder(
                CodeFromTokens::new(start_span, n_brackets),
            )),

            TTToken::BlockScopeOpen(start_span) => Ok(BuildStatus::StartInnerBuilder(
                BlockScopeFromTokens::new(py, start_span)?,
            )),

            TTToken::InlineScopeOpen(_) => {
                Ok(BuildStatus::StartInnerBuilder(InlineScopeFromTokens::new()))
            }

            TTToken::RawScopeOpen(start_span, n_opening) => Ok(BuildStatus::StartInnerBuilder(
                RawStringFromTokens::new(start_span, n_opening),
            )),

            // TODO open paragraph
            TTToken::Escaped(_, _) => todo!(),
            TTToken::Backslash(_) => todo!(),
            TTToken::OtherText(_) => todo!(),

            // TODO error close code without open
            TTToken::CodeClose(_, _) => todo!(),
            TTToken::CodeCloseOwningInline(_, _) => todo!(),
            TTToken::CodeCloseOwningRaw(_, _, _) => todo!(),
            TTToken::CodeCloseOwningBlock(_, _) => todo!(),

            // TODO error close raw scope without open
            TTToken::RawScopeClose(_, _) => todo!(),

            TTToken::ScopeClose(_) => on_close_scope(py, py_env, tok, data),
        }
    }
}

struct TopLevelDocumentBuilder {
    /// The current structure of the document, including toplevel content, segments, and the current block stacks (one block stack per included subfile)
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
impl BuildFromTokens for TopLevelDocumentBuilder {
    // This builder is responsible for spawning lower-level builders
    fn process_token(
        &mut self,
        py: Python,
        py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        process_block_level_token(
            py,
            py_env,
            tok,
            data,
            &mut self.expect_newline,
            |_, _, _, _| todo!("error close block scope without open"),
        )
    }

    // When EOF comes, we don't produce anything to bubble up - there's nothing above us!
    fn process_eof(
        &mut self,
        py: Python,
        py_env: &PyDict,
    ) -> TurnipTextContextlessResult<Option<PushToNextLevel>> {
        Ok(None)
    }

    fn process_push_from_inner_builder(
        &mut self,
        py: Python,
        py_env: &PyDict,
        pushed: Option<PushToNextLevel>,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        match pushed {
            Some(PushToNextLevel { from_builder, elem }) => match elem {
                DocElement::Header(header) => {
                    todo!("incorporate the new header into the InterimDocumentStructure")
                }
                DocElement::Block(block) => {
                    todo!("push the new block into the InterimDocumentStructure")
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
            TTToken::Newline(_) => Ok(BuildStatus::Done(None)),
            _ => Ok(BuildStatus::Continue),
        }
    }

    fn process_eof(
        &mut self,
        py: Python,
        py_env: &PyDict,
    ) -> TurnipTextContextlessResult<Option<PushToNextLevel>> {
        Ok(None)
    }

    fn process_push_from_inner_builder(
        &mut self,
        py: Python,
        py_env: &PyDict,
        pushed: Option<PushToNextLevel>,
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
    ) -> TurnipTextContextlessResult<BuildStatus> {
        panic!("RawStringFromTokens does not spawn inner builders")
    }

    fn process_eof(
        &mut self,
        py: Python,
        py_env: &PyDict,
    ) -> TurnipTextContextlessResult<Option<PushToNextLevel>> {
        todo!()
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
impl BuildFromTokens for BlockScopeFromTokens {
    fn process_token(
        &mut self,
        py: Python,
        py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        process_block_level_token(
            py,
            py_env,
            tok,
            data,
            &mut self.expect_newline,
            |_, _, tok, _| {
                if !self.ctx.try_extend(&tok) {
                    todo!("create some error to say 'closing block scope from different file'");
                }
                Ok(BuildStatus::Done(Some(self.ctx.make(DocElement::Block(
                    PyTcRef::of_unchecked(self.block_scope.as_ref(py)),
                )))))
            },
        )
    }

    fn process_push_from_inner_builder(
        &mut self,
        py: Python,
        py_env: &PyDict,
        pushed: Option<PushToNextLevel>,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        match pushed {
            Some(PushToNextLevel { from_builder, elem }) => match elem {
                DocElement::Header(header) => {
                    todo!("reject with error")
                }
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

    fn process_eof(
        &mut self,
        py: Python,
        py_env: &PyDict,
    ) -> TurnipTextContextlessResult<Option<PushToNextLevel>> {
        todo!("reject with eof in the middle of block scope")
    }
}

struct ParagraphFromTokens {
    ctx: BuilderContext,
}
impl ParagraphFromTokens {
    fn new_empty(py: Python, opening_span: ParseSpan) -> TurnipTextContextlessResult<Box<Self>> {
        Ok(Box::new(Self {
            ctx: BuilderContext::new("Paragraph", opening_span),
        }))
    }
    fn new_with_inline(
        py: Python,
        inline: &PyAny,
        inline_span: ParseSpan,
    ) -> TurnipTextContextlessResult<Box<Self>> {
        todo!()
    }
}
impl BuildFromTokens for ParagraphFromTokens {}

struct InlineScopeFromTokens {}
impl InlineScopeFromTokens {
    fn new() -> Box<Self> {
        Box::new(Self {})
    }
}
impl BuildFromTokens for InlineScopeFromTokens {}

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
    ) -> TurnipTextContextlessResult<BuildStatus> {
        todo!()
    }

    fn process_eof(
        &mut self,
        py: Python,
        py_env: &PyDict,
    ) -> TurnipTextContextlessResult<Option<PushToNextLevel>> {
        todo!("error EOF in the middle of code")
    }
}

fn py_internal_alloc<T: PyClass>(
    py: Python<'_>,
    value: impl Into<PyClassInitializer<T>>,
) -> TurnipTextContextlessResult<Py<T>> {
    Py::new(py, value).err_as_internal(py)
}
