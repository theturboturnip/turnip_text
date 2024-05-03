use pyo3::prelude::*;
use std::ffi::CString;
use thiserror::Error;

use crate::util::ParseContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserPythonCompileMode {
    EvalExpr,
    ExecStmts,
    ExecIndentedStmts,
}

/// The contexts in which you might execute Python on user-generated code or objects
#[derive(Error, Debug)]
pub enum TTUserPythonError {
    // Considered throwing an error here, but I'm not convinced it's common enough to try and detect.
    // If using e.g. template-generated turnip-text it might be useful?
    // It would certainly be unholy, but I don't think this error would make it better.
    // #[error("Found empty eval-brackets, likely not what was intended")]
    // EmptyEvalBrackets { code_ctx: ParseContext },
    /// Compiling user-supplied code
    #[error("Error when compiling Python from eval-brackets in mode {mode:?}: {err}")]
    CompilingEvalBrackets {
        code_ctx: ParseContext,
        code: CString,
        mode: UserPythonCompileMode,
        err: PyErr,
    },
    /// Directly running user-supplied code
    #[error("Error when executing code from eval-brackets: {err}")]
    RunningEvalBrackets {
        code_ctx: ParseContext,
        code: CString,
        mode: UserPythonCompileMode,
        err: PyErr,
    },
    /// Ran user code from an eval-bracket which didn't have an argument attached,
    /// failed to coerce the code result to Block, Inline, Header, or TurnipTextSource
    #[error(
        "Successfully evaluated eval-brackets, but the output was not None, a TurnipTextSource, a \
         Header, a Block, or coercible to Inline"
    )]
    CoercingNonBuilderEvalBracket {
        code_ctx: ParseContext,
        obj: PyObject,
    },
    /// Ran user code from an eval-bracket which was followed by a block scope argument,
    /// but failed to coerce the code result to BlockScopeBuilder
    #[error(
        "Successfully evaluated eval-brackets, constructed a block-scope to provide to a builder, \
         but raised an error when building the inline scope: {err}"
    )]
    CoercingBlockScopeBuilder {
        code_ctx: ParseContext,
        obj: PyObject,
        err: PyErr,
    },
    /// Ran user code from an eval-bracket which was followed by an inline scope argument,
    /// but failed to coerce the code result to InlineScopeBuilder
    #[error(
        "Successfully evaluated eval-brackets, constructed an inline-scope to provide to a \
         builder, but raised an error when building the inline scope: {err}"
    )]
    CoercingInlineScopeBuilder {
        code_ctx: ParseContext,
        obj: PyObject,
        err: PyErr,
    },
    /// Ran user code from an eval-bracket which was followed by a raw scope argument,
    /// but failed to coerce the code result to RawScopeBuilder
    #[error(
        "Successfully evaluated eval-brackets, constructed a raw-scope to provide to a builder, \
         but the eval-bracket output was not a RawScopeBuilder: {err}"
    )]
    CoercingRawScopeBuilder {
        code_ctx: ParseContext,
        obj: PyObject,
        err: PyErr,
    },
}
