use std::{cell::RefCell, rc::Rc};

use pyo3::{exceptions::PySyntaxError, ffi::Py_None, intern, prelude::*, types::PyDict};

use crate::{
    error::{
        interp::{InterpError, MapContextlessResult},
        TurnipTextContextlessResult, UserPythonExecError,
    },
    lexer::TTToken,
    python::{
        interop::{
            coerce_to_inline_pytcref, BlockScopeBuilder, BuilderOutcome, InlineScopeBuilder,
            RawScopeBuilder, TurnipTextSource,
        },
        typeclass::PyTcRef,
    },
    util::{ParseContext, ParseSpan},
};

use super::{
    ambiguous_scope::BlockLevelAmbiguousScope, inline::RawStringFromTokens, rc_refcell, BlockElem,
    BuildFromTokens, BuildStatus, DocElement, InlineElem, PushToNextLevel,
};

pub struct CodeFromTokens {
    ctx: ParseContext,
    n_closing: usize,
    code: String,
    evaled_code: Option<PyObject>,
}
impl CodeFromTokens {
    pub fn new(start_span: ParseSpan, n_opening: usize) -> Rc<RefCell<Self>> {
        rc_refcell(Self {
            ctx: ParseContext::new(start_span, start_span),
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
                        let res: &PyAny = eval_or_exec(py, py_env, &self.code).map_err(|err| {
                            UserPythonExecError::RunningEvalBrackets {
                                code: self.ctx,
                                err,
                            }
                        })?;

                        // If we evaluated a TurnipTextSource, it may not be a builder of any kind thus we can finish immediately.
                        if let Ok(inserted_file) = res.extract::<TurnipTextSource>() {
                            Ok(BuildStatus::DoneAndNewSource(self.ctx, inserted_file))
                        } else {
                            self.evaled_code = Some(res.into_py(py));
                            Ok(BuildStatus::Continue)
                        }
                    }
                    TTToken::EOF(eof_span) => Err(InterpError::EndedInsideCode {
                        code_start: self.ctx.first_tok(),
                        eof_span,
                    })?,
                    _ => {
                        // Code blocks use raw stringification to avoid confusion between text written and text entered
                        self.code.push_str(tok.stringify_raw(data));
                        Ok(BuildStatus::Continue)
                    }
                }
            }
            // Parse one token after the code ends to see what we should do.
            Some(evaled_result) => match tok {
                // A scope open could be for a block scope or inline scope - we accept either, so use the BlockLevelAmbiguousScope
                TTToken::ScopeOpen(start_span) => Ok(BuildStatus::StartInnerBuilder(
                    BlockLevelAmbiguousScope::new(start_span),
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
                        Ok(BuildStatus::DoneAndReprocessToken(Some((
                            self.ctx,
                            DocElement::HeaderFromCode(header),
                        ))))
                    } else if let Ok(block) = PyTcRef::of(evaled_result_ref) {
                        Ok(BuildStatus::DoneAndReprocessToken(Some((
                            self.ctx,
                            BlockElem::FromCode(block).into(),
                        ))))
                    } else {
                        let inline =
                            coerce_to_inline_pytcref(py, evaled_result_ref).map_err(|_err| {
                                UserPythonExecError::CoercingNonBuilderEvalBracket {
                                    code: self.ctx,
                                    obj: evaled_result.clone_ref(py),
                                }
                            })?;
                        Ok(BuildStatus::DoneAndReprocessToken(Some((
                            self.ctx,
                            InlineElem::FromCode(inline).into(),
                        ))))
                    }
                }
            },
        }
    }

    fn process_push_from_inner_builder(
        &mut self,
        py: Python,
        _py_env: &PyDict,
        pushed: Option<PushToNextLevel>,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        let evaled_result_ref = self.evaled_code.take().unwrap().into_ref(py);

        let (elem_ctx, elem) = pushed.expect("Should never get a built None - CodeFromTokens only spawns BlockScopeFromTokens, InlineScopeFromTokens, RawScopeFromTokens none of which return None.");
        let built = match elem {
            DocElement::Block(BlockElem::BlockScope(blocks)) => {
                let builder: PyTcRef<BlockScopeBuilder> =
                    PyTcRef::of_friendly(evaled_result_ref, "value returned by eval-bracket")
                        .map_err(|err| UserPythonExecError::CoercingBlockScopeBuilder {
                            code: self.ctx,
                            obj: evaled_result_ref.to_object(py),
                            err,
                        })?;

                // Now that we know coersion is a success, update the code span
                assert!(
                    self.ctx.try_combine(elem_ctx),
                    "Code got a built object from a different file that it was opened in"
                );

                BlockScopeBuilder::call_build_from_blocks(py, builder, blocks)
                    .err_as_internal(py)?
            }
            DocElement::Inline(InlineElem::InlineScope(inlines)) => {
                let builder: PyTcRef<InlineScopeBuilder> =
                    PyTcRef::of_friendly(evaled_result_ref, "value returned by eval-bracket")
                        .map_err(|err| UserPythonExecError::CoercingInlineScopeBuilder {
                            code: self.ctx,
                            obj: evaled_result_ref.to_object(py),
                            err,
                        })?;

                // Now that we know coersion is a success, update the code span
                assert!(
                    self.ctx.try_combine(elem_ctx),
                    "Code got a built object from a different file that it was opened in"
                );

                InlineScopeBuilder::call_build_from_inlines(py, builder, inlines)
                    .err_as_internal(py)?
            }
            DocElement::Inline(InlineElem::Raw(raw)) => {
                let builder: PyTcRef<RawScopeBuilder> =
                    PyTcRef::of_friendly(evaled_result_ref, "value returned by eval-bracket")
                        .map_err(|err| UserPythonExecError::CoercingRawScopeBuilder {
                            code: self.ctx,
                            obj: evaled_result_ref.to_object(py),
                            err,
                        })?;

                // Now that we know coersion is a success, update the code span
                assert!(
                    self.ctx.try_combine(elem_ctx),
                    "Code got a built object from a different file that it was opened in"
                );

                RawScopeBuilder::call_build_from_raw(py, &builder, raw.borrow(py).0.clone_ref(py))
                    .err_as_internal(py)?
            }
            _ => unreachable!("Invalid combination of requested and actual built element types"),
        };
        match built {
            BuilderOutcome::Block(block) => Ok(BuildStatus::Done(Some((
                self.ctx,
                BlockElem::FromCode(block).into(),
            )))),
            BuilderOutcome::Inline(inline) => Ok(BuildStatus::Done(Some((
                self.ctx,
                InlineElem::FromCode(inline).into(),
            )))),
            BuilderOutcome::Header(header) => Ok(BuildStatus::Done(Some((
                self.ctx,
                DocElement::HeaderFromCode(header),
            )))),
            BuilderOutcome::None => Ok(BuildStatus::Done(None)),
        }
    }

    fn on_emitted_source_inside(
        &mut self,
        _code_emitting_source: ParseContext,
    ) -> TurnipTextContextlessResult<()> {
        unreachable!("CodeFromTokens does not spawn an inner code builder, so cannot have a source file emitted inside")
    }
    fn on_emitted_source_closed(&mut self, _inner_source_emitted_by: ParseSpan) {
        unreachable!("CodeFromTokens does not spawn an inner code builder, so cannot have a source file emitted inside")
    }
}

pub fn eval_or_exec<'py, 'code>(
    py: Python<'py>,
    py_env: &'py PyDict,
    code: &'code str,
) -> PyResult<&'py PyAny> {
    // Python picks up leading whitespace as an incorrect indent.
    let code = code.trim();
    // In exec() contexts it would be really nice to allow a toplevel indent (ignoring blank lines when calculating it)
    // This requires two steps: determining if there is an indent, and removing that indent.
    // Detecting an indent could be done programmatically *if* PyO3 exposed PyIndentError like it does PySyntaxError,
    // otherwise we'd have to do our own parsing. The branch manual-indent-injection has some initial regexes for this,
    // but I realized that doing this detection also requires ignoring comments (they don't count towards indentation)
    // and I thought that strays too close to actually parsing the python for my liking.
    // Resolving the indent is another problem: a feature automatically "dedenting" code passed to python -c
    // exists https://discuss.python.org/t/allowing-indented-code-for-c/44122/1 and proposes appending `if True:` to the start as a stopgap.
    // textwrap.dedent() also exists but I don't think it's suitable for code.
    // Thus I have abandoned this quest.
    let raw_res = match py.eval(code, Some(py_env), None) {
        Ok(raw_res) => raw_res,
        Err(error) if error.is_instance_of::<PySyntaxError>(py) => {
            // Try to exec() it as a statement instead of eval() it as an expression
            py.run(code, Some(py_env), None)?;
            // Acquire a Py<PyAny> to Python None, then use into_ref() to convert it into a &PyAny.
            // This should optimize down to `*Py_None()` because Py<T> and PyAny both boil down to *ffi::Py_Object;
            // This is so that places that *require* non-None (e.g. NeedBlockBuilder) will always raise an error in the following match statement.
            // This is safe because Py_None() returns a pointer-to-static.
            unsafe { Py::<PyAny>::from_borrowed_ptr(py, Py_None()).into_ref(py) }
        }
        Err(error) => return Err(error),
    };
    // If it has __get__, call it.
    // `property` objects and other data descriptors use this.
    let getter = intern!(py, "__get__");
    if raw_res.hasattr(getter)? {
        // https://docs.python.org/3.8/howto/descriptor.html
        // "For objects, the machinery is in object.__getattribute__() which transforms b.x into type(b).__dict__['x'].__get__(b, type(b))."
        //
        // We're transforming `[x]` into (effectively) `py_env.x`
        // which should transform into (type(py_env).__dict__['x']).__get__(py_env, type(py_env))
        // = raw_res.__get__(py_env, type(py_env))
        raw_res.call_method1(getter, (py_env, py_env.get_type()))
    } else {
        Ok(raw_res)
    }
}
