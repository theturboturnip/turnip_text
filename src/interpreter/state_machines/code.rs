use std::ffi::CString;

use pyo3::{exceptions::PySyntaxError, prelude::*, types::PyDict};

use crate::{
    interpreter::{
        error::{
            syntax::TTSyntaxError,
            user_python::{TTUserPythonError, UserPythonCompileMode},
            TTResult,
        },
        lexer::TTToken,
        ParserEnv,
    },
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
    ambiguous_scope::AmbiguousScopeProcessor, inline::RawStringProcessor, rc_refcell, BlockElem,
    DocElement, EmittedElement, InlineElem, ProcStatus, TokenProcessor,
};

pub struct CodeProcessor {
    ctx: ParseContext,
    n_closing: usize,
    code: String,
    evaled_code: Option<PyObject>,
}
impl CodeProcessor {
    pub fn new(start_span: ParseSpan, n_opening: usize) -> Self {
        Self {
            ctx: ParseContext::new(start_span, start_span),
            n_closing: n_opening,
            code: String::new(),
            evaled_code: None,
        }
    }
}
impl TokenProcessor for CodeProcessor {
    fn process_token(
        &mut self,
        py: Python,
        py_env: ParserEnv,
        tok: TTToken,
        data: &str,
    ) -> TTResult<ProcStatus> {
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
                        let eval_obj: Bound<'_, PyAny> =
                            eval_or_exec(py, py_env, &self.code, self.ctx)?;

                        // If we evaluated a TurnipTextSource, it may not be a builder of any kind thus we can finish immediately.
                        if let Ok(inserted_file) = eval_obj.extract::<TurnipTextSource>() {
                            Ok(ProcStatus::PopAndNewSource(self.ctx, inserted_file))
                        } else {
                            // Save the evaled object.
                            // Keep going so we can peek at the next token,
                            // to see if we need to attach a scope to this object.
                            self.evaled_code = Some(eval_obj.into_py(py));
                            Ok(ProcStatus::Continue)
                        }
                    }
                    TTToken::EOF(eof_span) => Err(TTSyntaxError::EndedInsideCode {
                        code_start: self.ctx.first_tok(),
                        eof_span,
                    })?,
                    _ => {
                        // Code blocks use raw stringification to avoid confusion between text written and text entered
                        self.code.push_str(tok.stringify_raw(data));
                        Ok(ProcStatus::Continue)
                    }
                }
            }
            // Parse one token after the code ends to see what we should do.
            Some(evaled_result) => match tok {
                // A scope open could be for a block scope or inline scope - we accept either, so use the BlockLevelAmbiguousScope
                TTToken::ScopeOpen(start_span) => Ok(ProcStatus::PushProcessor(rc_refcell(
                    AmbiguousScopeProcessor::new(start_span),
                ))),
                TTToken::RawScopeOpen(start_span, n_opening) => Ok(ProcStatus::PushProcessor(
                    rc_refcell(RawStringProcessor::new(start_span, n_opening)),
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

                    let evaled_result_ref = evaled_result.bind(py);

                    if evaled_result_ref.is_none() {
                        Ok(ProcStatus::PopAndReprocessToken(None))
                    } else if let Ok(header) = PyTcRef::of(evaled_result_ref) {
                        Ok(ProcStatus::PopAndReprocessToken(Some((
                            self.ctx,
                            DocElement::HeaderFromCode(header),
                        ))))
                    } else if let Ok(block) = PyTcRef::of(evaled_result_ref) {
                        Ok(ProcStatus::PopAndReprocessToken(Some((
                            self.ctx,
                            BlockElem::FromCode(block).into(),
                        ))))
                    } else {
                        let inline =
                            coerce_to_inline_pytcref(py, evaled_result_ref).map_err(|_err| {
                                TTUserPythonError::CoercingNonBuilderEvalBracket {
                                    code_ctx: self.ctx,
                                    obj: evaled_result.clone_ref(py),
                                }
                            })?;
                        Ok(ProcStatus::PopAndReprocessToken(Some((
                            self.ctx,
                            InlineElem::FromCode(inline).into(),
                        ))))
                    }
                }
            },
        }
    }

    fn process_emitted_element(
        &mut self,
        py: Python,
        _py_env: ParserEnv,
        pushed: Option<EmittedElement>,
    ) -> TTResult<ProcStatus> {
        let evaled_result_ref = self.evaled_code.take().unwrap().into_bound(py);

        let (elem_ctx, elem) = pushed.expect(
            "Should never get a built None - CodeProcessor only spawns AmbiguousScopeProcessor \
             and RawScopeProcessor none of which return None.",
        );
        let built = match elem {
            DocElement::Block(BlockElem::BlockScope(blocks)) => {
                let builder: PyTcRef<BlockScopeBuilder> =
                    PyTcRef::of_friendly(&evaled_result_ref, "value returned by eval-bracket")
                        .map_err(|err| TTUserPythonError::CoercingBlockScopeBuilder {
                            code_ctx: self.ctx,
                            obj: evaled_result_ref.to_object(py),
                            err,
                        })?;

                // Now that we know coersion is a success, update the code span
                assert!(
                    self.ctx.try_combine(elem_ctx),
                    "Code got a built object from a different file that it was opened in"
                );

                BlockScopeBuilder::call_build_from_blocks(py, builder, blocks)?
            }
            DocElement::Inline(InlineElem::InlineScope(inlines)) => {
                let builder: PyTcRef<InlineScopeBuilder> =
                    PyTcRef::of_friendly(&evaled_result_ref, "value returned by eval-bracket")
                        .map_err(|err| TTUserPythonError::CoercingInlineScopeBuilder {
                            code_ctx: self.ctx,
                            obj: evaled_result_ref.to_object(py),
                            err,
                        })?;

                // Now that we know coersion is a success, update the code span
                assert!(
                    self.ctx.try_combine(elem_ctx),
                    "Code got a built object from a different file that it was opened in"
                );

                InlineScopeBuilder::call_build_from_inlines(py, builder, inlines)?
            }
            DocElement::Inline(InlineElem::Raw(raw)) => {
                let builder: PyTcRef<RawScopeBuilder> =
                    PyTcRef::of_friendly(&evaled_result_ref, "value returned by eval-bracket")
                        .map_err(|err| TTUserPythonError::CoercingRawScopeBuilder {
                            code: self.ctx,
                            obj: evaled_result_ref.to_object(py),
                            err,
                        })?;

                // Now that we know coersion is a success, update the code span
                assert!(
                    self.ctx.try_combine(elem_ctx),
                    "Code got a built object from a different file that it was opened in"
                );

                RawScopeBuilder::call_build_from_raw(py, builder, raw.borrow(py).0.clone_ref(py))?
            }
            _ => unreachable!("Invalid combination of requested and actual built element types"),
        };
        match built {
            BuilderOutcome::Block(block) => Ok(ProcStatus::Pop(Some((
                self.ctx,
                BlockElem::FromCode(block).into(),
            )))),
            BuilderOutcome::Inline(inline) => Ok(ProcStatus::Pop(Some((
                self.ctx,
                InlineElem::FromCode(inline).into(),
            )))),
            BuilderOutcome::Header(header) => Ok(ProcStatus::Pop(Some((
                self.ctx,
                DocElement::HeaderFromCode(header),
            )))),
            BuilderOutcome::None => Ok(ProcStatus::Pop(None)),
        }
    }

    fn on_emitted_source_inside(&mut self, _code_emitting_source: ParseContext) -> TTResult<()> {
        unreachable!(
            "CodeProcessor does not spawn an inner code builder, so cannot have a source file \
             emitted inside"
        )
    }
    fn on_emitted_source_closed(&mut self, _inner_source_emitted_by: ParseSpan) {
        unreachable!(
            "CodeProcessor does not spawn an inner code builder, so cannot have a source file \
             emitted inside"
        )
    }
}

/// Tries to compile the given code for the given compile mode.
/// If the compilation succeeds, runs the code.
///
/// Translates compile errors to [UserPythonExecError::CompilingEvalBrackets].
/// Translates run errors to [UserPythonExecError::RunningEvalBrackets].
fn try_compile_and_run<'py>(
    py: Python<'py>,
    py_env: &'py Bound<'py, PyDict>,
    code: &CString,
    code_ctx: ParseContext,
    mode: UserPythonCompileMode,
) -> Result<Bound<'py, PyAny>, TTUserPythonError> {
    let compile_mode = match mode {
        UserPythonCompileMode::EvalExpr => pyo3::ffi::Py_eval_input,
        UserPythonCompileMode::ExecStmts | UserPythonCompileMode::ExecIndentedStmts => {
            pyo3::ffi::Py_file_input
        }
    };

    unsafe {
        let code_obj =
            pyo3::ffi::Py_CompileString(code.as_ptr(), "<string>\0".as_ptr() as _, compile_mode);
        if code_obj.is_null() {
            return Err(TTUserPythonError::CompilingEvalBrackets {
                code_ctx,
                code: code.clone(),
                mode,
                err: PyErr::fetch(py),
            });
        }
        let globals = py_env.as_ptr();
        let locals = globals;
        let res_ptr = pyo3::ffi::PyEval_EvalCode(code_obj, globals, locals);
        pyo3::ffi::Py_DECREF(code_obj);

        let res: PyResult<Bound<'_, PyAny>> = Bound::from_owned_ptr_or_err(py, res_ptr);
        // Make sure exec-mode compilations always return None
        match (mode, &res) {
            (
                UserPythonCompileMode::ExecStmts | UserPythonCompileMode::ExecIndentedStmts,
                Ok(exec_obj),
            ) => debug_assert!(exec_obj.is_none()),
            _ => {}
        }
        res.map_err(|run_pyerr| TTUserPythonError::RunningEvalBrackets {
            code_ctx,
            code: code.clone(),
            mode,
            err: run_pyerr,
        })
    }
}

pub fn eval_or_exec<'py, 'code>(
    py: Python<'py>,
    py_env: &'py Bound<'py, PyDict>,
    code: &'code str,
    code_ctx: ParseContext,
) -> Result<Bound<'py, PyAny>, TTUserPythonError> {
    // The turnip-text lexer rejects the nul-byte so it cannot be found in the code.
    let code_trimmed =
        CString::new(code.trim()).expect("Nul-byte should not be present inside code");

    // First, try to compile the code as a single Python expression.
    // If that succeeds to compile, run it and return the result.
    match try_compile_and_run(
        py,
        py_env,
        &code_trimmed,
        code_ctx,
        UserPythonCompileMode::EvalExpr,
    ) {
        // If compiling the code in eval mode gave a SyntaxError, try compiling in exec mode.
        // Compile the trimmed person first so we can do e.g. `[ x = 5 ]`
        Err(TTUserPythonError::CompilingEvalBrackets { err, .. })
            if err.is_instance_of::<PySyntaxError>(py) =>
        {
            match try_compile_and_run(py, py_env, &code_trimmed, code_ctx, UserPythonCompileMode::ExecStmts) {
                    // Can't use .is_instance_of::<PyIndentationError> because PyO3 doesn't generate a PyIndentationError type.
                    Err(TTUserPythonError::CompilingEvalBrackets { err, .. })
                    // I feel fine expecting Py_CompileString to raise an error with a type with a name.
                        if err
                            .get_type_bound(py)
                            .name()
                            .expect("Failed to get compile error type name")
                            == "builtins.IndentationError" =>
                    {
                        // Compiling the code in exec mode gave an IndentationError.
                        // Put an if True:\n in front and see if that helps.

                        let code_with_indent_guard = {
                            // The turnip-text lexer rejects the nul-byte so it cannot be found in the code.
                            unsafe {
                                CString::from_vec_with_nul_unchecked(
                                    format!("if True:\n{code}\0").into_bytes(),
                                )
                            }
                        };
                        try_compile_and_run(
                            py,
                            py_env,
                            &code_with_indent_guard,
                            code_ctx,
                            UserPythonCompileMode::ExecIndentedStmts,
                        )
                    }
                    other => other
                }
        }
        other => other,
    }
}
