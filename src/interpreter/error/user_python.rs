use pyo3::prelude::*;
use std::ffi::CString;
use thiserror::Error;

use crate::util::{ParseContext, ParseSpan};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserPythonCompileMode {
    EvalExpr,
    ExecStmts,
    ExecIndentedStmts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserPythonBuildMode {
    FromBlock,
    FromInline,
    FromRaw,
}

/// The contexts in which you might execute Python on user-generated code or objects
#[derive(Error, Debug)]
pub enum TTUserPythonError {
    // Considered throwing an error here, but I'm not convinced it's common enough to try and detect.
    // If using e.g. template-generated turnip_text it might be useful?
    // It would certainly be unholy, but I don't think this error would make it better.
    // #[error("Found empty eval-brackets, likely not what was intended")]
    // EmptyEvalBrackets { code_ctx: ParseContext },
    /// Compiling user-supplied code
    #[error("Error when compiling Python from eval-brackets in mode {mode:?}: {err}")]
    CompilingEvalBrackets {
        code_ctx: ParseContext,
        /// The number of '-' characters inside `[--- ---]`
        code_n_hyphens: usize,
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
    CoercingEvalBracketToElement {
        code_ctx: ParseContext,
        obj: PyObject,
        err: PyErr,
    },
    /// Ran user code from an eval-bracket, needed that to be a builder, didn't get it
    #[error(
            "Successfully evaluated eval-brackets, attached scope implied {build_mode:?} builder but the eval-bracket output didn't fit: {err}"
        )]
    CoercingEvalBracketToBuilder {
        code_ctx: ParseContext,
        scope_open: ParseSpan,
        obj: PyObject,
        build_mode: UserPythonBuildMode,
        err: PyErr,
    },
    /// Ran user code from an eval-bracket which was followed by a scope argument,
    /// but an error was raised while building
    #[error(
        "Successfully evaluated eval-brackets, constructed an argument to provide to a \
         builder {build_mode:?}, but raised an error when building: {err}"
    )]
    Building {
        code_ctx: ParseContext,
        arg_ctx: ParseContext,
        builder: PyObject,
        build_mode: UserPythonBuildMode,
        err: PyErr,
    },
    /// Ran user code from an eval-bracket which had an argument attached,
    /// successfully used the argument to build something, but
    /// the result was not None, Block, Inline, or Header
    #[error(
        "Successfully evaluated eval-brackets and built an object with the result, \
        but the output was not None, a Header, a Block, or an Inline"
    )]
    CoercingBuildResultToElement {
        code_ctx: ParseContext,
        arg_ctx: ParseContext,
        // FUTURE build_mode
        builder: PyObject,
        obj: PyObject,
        err: PyErr,
    },
}
