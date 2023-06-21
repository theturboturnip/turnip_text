use pyo3::{Python, types::PyDict, PyResult};

use crate::{python::{typeclass::PyTcRef, interop::{BlockScopeBuilder, InlineScopeBuilder, RawScopeBuilder, Block, Inline, InlineXorBlock}}, lexer::TTToken, util::ParseSpan};

use super::{MapInterpResult, InterpResult};

pub enum EvalBracketResult {
    BlockBuilder(PyTcRef<BlockScopeBuilder>),
    InlineBuilder(PyTcRef<InlineScopeBuilder>),
    Raw(PyTcRef<RawScopeBuilder>, usize),
    Block(PyTcRef<Block>),
    Inline(PyTcRef<Inline>),
}
impl EvalBracketResult {
    pub fn eval_in_correct_ctx(
        py: Python,
        globals: &PyDict,
        code: &str,
        tok: TTToken,
    ) -> PyResult<EvalBracketResult> {
        // Python picks up leading whitespace as an incorrect indent
        let code = code.trim();
        let raw_res = py.eval(code, Some(globals), None)?;
        let res = match tok {
            TTToken::CodeCloseOwningBlock(_, _) => EvalBracketResult::BlockBuilder(PyTcRef::of(raw_res)?),
            TTToken::CodeCloseOwningInline(_, _) => {
                EvalBracketResult::InlineBuilder(PyTcRef::of(raw_res)?)
            }
            TTToken::CodeCloseOwningRaw(_, _, n_hashes) => {
                EvalBracketResult::Raw(PyTcRef::of(raw_res)?, n_hashes)
            }
            TTToken::CodeClose(_, _) => {
                // See if it's either a Block or an Inline. It must be exactly one of them.
                let _: PyTcRef<InlineXorBlock> = PyTcRef::of(raw_res)?;
                // If so, produce either.
                if let Ok(block) = PyTcRef::of(raw_res) {
                    EvalBracketResult::Block(block)
                } else if let Ok(inl) = PyTcRef::of(raw_res) {
                    EvalBracketResult::Inline(inl)
                } else {
                    unreachable!()
                }
            },
            _ => unreachable!(),
        };
        Ok(res)
    }
}


/// If the code is closed, evaluates the result and checks it matches the type of code close:
/// - [TTToken::CodeCloseOwningBlock] -> [EvalBracketResult::Block]
/// - [TTToken::CodeCloseOwningInline] -> [EvalBracketResult::Inline]
/// - [TTToken::CodeCloseOwningRaw] -> [EvalBracketResult::Inline]
/// - [TTToken::CodeClose] -> [EvalBracketResult::Other]
pub fn handle_code_mode(
    data: &str,
    tok: TTToken,
    code: &mut String,
    code_start: &ParseSpan,
    expected_close_len: usize,
    py: Python,
    globals: &PyDict,
) -> InterpResult<Option<(EvalBracketResult, ParseSpan)>> {
    let code_span = match tok {
        TTToken::CodeClose(close_span, n)
        | TTToken::CodeCloseOwningBlock(close_span, n)
        | TTToken::CodeCloseOwningInline(close_span, n)
        | TTToken::CodeCloseOwningRaw(close_span, n, _)
            if n == expected_close_len =>
        {
            ParseSpan {
                start: code_start.start,
                end: close_span.end,
            }
        }
        _ => {
            // Code blocks use raw stringification to avoid confusion between text written and text entered
            code.push_str(tok.stringify_raw(data));
            return Ok(None);
        }
    };

    let res = EvalBracketResult::eval_in_correct_ctx(py, globals, code, tok)
        .err_as_interp(py, code_span)?;
    Ok(Some((res, code_span)))
}