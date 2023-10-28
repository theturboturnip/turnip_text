use pyo3::{
    exceptions::PySyntaxError, ffi::Py_None, intern, types::PyDict, Py, PyAny, PyResult, Python,
};

use crate::{
    lexer::TTToken,
    python::{
        interop::{
            coerce_to_inline_pytcref, Block, BlockScopeBuilder, DocSegmentHeader, Inline,
            InlineScopeBuilder, InsertedFile, RawScopeBuilder,
        },
        typeclass::PyTcRef,
    },
    util::ParseSpan,
};

use super::{InterpResult, MapInterpResult};

pub enum EvalBracketContext {
    NeedBlockBuilder,
    NeedInlineBuilder,
    NeedRawBuilder { n_hashes: usize },
    WantNonBuilder,
}
pub enum EvalBracketResult {
    /// A BlockScopeBuilder which was Needed because the final token was a [TTToken::CodeCloseOwningBlock]
    NeededBlockBuilder(PyTcRef<BlockScopeBuilder>),
    /// An InlineScopeBuilder which was Needed because the final token was a [TTToken::CodeCloseOwningInline]
    NeededInlineBuilder(PyTcRef<InlineScopeBuilder>),
    /// A RawScopeBuilder which was Needed because the final token was a [TTToken::CodeCloseOwningRaw]
    NeededRawBuilder(PyTcRef<RawScopeBuilder>, usize),
    /// An object implementing DocSegmentHeader
    DocSegmentHeader(PyTcRef<DocSegmentHeader>),
    /// An object implementing Block
    Block(PyTcRef<Block>),
    /// An object implementing Inline, or which was coerced to something implementing Inline
    Inline(PyTcRef<Inline>),
    /// A InsertedFile object
    InsertedFile(InsertedFile),
    /// None - either because it was an exec statement (e.g. `[x = 5]`) or because it genuinely was none (e.g. `[None]`)
    PyNone,
}
impl EvalBracketResult {
    pub fn eval_in_ctx(
        py: Python,
        py_env: &PyDict,
        code: &str,
        ctx: EvalBracketContext,
    ) -> PyResult<EvalBracketResult> {
        // Python picks up leading whitespace as an incorrect indent
        let code = code.trim();
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
        let raw_res = if raw_res.hasattr(getter)? {
            // https://docs.python.org/3.8/howto/descriptor.html
            // "For objects, the machinery is in object.__getattribute__() which transforms b.x into type(b).__dict__['x'].__get__(b, type(b))."
            //
            // We're transforming `[x]` into (effectively) `py_env.x`
            // which should transform into (type(py_env).__dict__['x']).__get__(py_env, type(py_env))
            // = raw_res.__get__(py_env, type(py_env))
            raw_res.call_method1(getter, (py_env, py_env.get_type()))?
        } else {
            raw_res
        };
        let res = match ctx {
            EvalBracketContext::NeedBlockBuilder => {
                EvalBracketResult::NeededBlockBuilder(PyTcRef::of(raw_res)?)
            }
            EvalBracketContext::NeedInlineBuilder => {
                EvalBracketResult::NeededInlineBuilder(PyTcRef::of(raw_res)?)
            }
            EvalBracketContext::NeedRawBuilder { n_hashes } => {
                EvalBracketResult::NeededRawBuilder(PyTcRef::of(raw_res)?, n_hashes)
            }
            EvalBracketContext::WantNonBuilder => {
                if raw_res.is_none() {
                    EvalBracketResult::PyNone
                } else {
                    // Consider: we may have an object at the very start of the line.
                    // If it's an Inline, e.g. "[virtio] is a thing..." then we want to return Inline so the rest of the line can be added.
                    // If it's a Block, e.g. [image_figure(...)], then we want to return Block.
                    // If it's neither, it needs to be *coerced*.
                    // But what should coercion look like? What should we try to coerce the object *to*?
                    // Well, what can be coerced?
                    // Coercible to inline:
                    // - Inline        -> `x`
                    // - List[Inline]  -> `InlineScope(x)`
                    // - str/float/int -> `UnescapedText(str(x))`
                    // Coercible to block:
                    // - Block             -> `x`
                    // - List[Block]       -> `BlockScope(x)`
                    // - Sentence          -> `Paragraph([x])
                    // - CoercibleToInline -> `Paragraph([Sentence([coerce_to_inline(x)])])`
                    // I do not see the need to allow eval-brackets to directly return List[Block] or Sentence at all.
                    // Similar outcomes can be acheived by wrapping in BlockScope or Paragraph manually in the evaluated code, which better demonstrates intent.
                    // If we always coerce to inline, then the wrapping in Paragraph and Sentence happens naturally in the interpreter.
                    // => We check if it's a block, and if it isn't we try to coerce to inline.

                    // If they return an InsertedFile then just do that.
                    if let Ok(inserted_file) = raw_res.extract::<InsertedFile>() {
                        EvalBracketResult::InsertedFile(inserted_file)
                    } else if let Ok(doc_seg) = PyTcRef::of(raw_res) {
                        EvalBracketResult::DocSegmentHeader(doc_seg)
                    } else if let Ok(blk) = PyTcRef::of(raw_res) {
                        EvalBracketResult::Block(blk)
                    } else {
                        EvalBracketResult::Inline(coerce_to_inline_pytcref(py, raw_res)?)
                    }
                }
            }
        };
        Ok(res)
    }
}

/// When eval-brackets are closed, evaluates the result and checks it matches the type of close:
/// - [TTToken::CodeCloseOwningBlock] -> block builder
/// - [TTToken::CodeCloseOwningInline] -> inline builder
/// - [TTToken::CodeCloseOwningRaw] -> raw builder
/// - [TTToken::CodeClose] -> block | inline | none
pub fn eval_brackets(
    data: &str,
    tok: TTToken,
    code: &mut String,
    code_start: &ParseSpan,
    expected_close_len: usize,
    py: Python,
    py_env: &PyDict,
) -> InterpResult<Option<(EvalBracketResult, ParseSpan)>> {
    let (code_span, eval_ctx) = match tok {
        TTToken::CodeClose(close_span, n_close_brackets)
            if n_close_brackets == expected_close_len =>
        {
            (
                ParseSpan {
                    start: code_start.start,
                    end: close_span.end,
                },
                EvalBracketContext::WantNonBuilder,
            )
        }
        TTToken::CodeCloseOwningBlock(close_span, n_close_brackets)
            if n_close_brackets == expected_close_len =>
        {
            (
                ParseSpan {
                    start: code_start.start,
                    end: close_span.end,
                },
                EvalBracketContext::NeedBlockBuilder,
            )
        }
        TTToken::CodeCloseOwningInline(close_span, n_close_brackets)
            if n_close_brackets == expected_close_len =>
        {
            (
                ParseSpan {
                    start: code_start.start,
                    end: close_span.end,
                },
                EvalBracketContext::NeedInlineBuilder,
            )
        }
        TTToken::CodeCloseOwningRaw(close_span, n_close_brackets, n_hashes)
            if n_close_brackets == expected_close_len =>
        {
            (
                ParseSpan {
                    start: code_start.start,
                    end: close_span.end,
                },
                EvalBracketContext::NeedRawBuilder { n_hashes },
            )
        }

        _ => {
            // Code blocks use raw stringification to avoid confusion between text written and text entered
            code.push_str(tok.stringify_raw(data));
            return Ok(None);
        }
    };

    let res =
        EvalBracketResult::eval_in_ctx(py, py_env, code, eval_ctx).err_as_interp(py, code_span)?;
    Ok(Some((res, code_span)))
}
