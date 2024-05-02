use std::ffi::CString;

use annotate_snippets::display_list::DisplayList;
use pyo3::{
    types::{PyAnyMethods, PyStringMethods, PyTypeMethods},
    PyErr, PyObject, Python,
};
use thiserror::Error;

use crate::{interpreter::ParsingFile, util::ParseContext};

use self::interp::InterpError;

mod display;
pub mod interp;

pub fn stringify_pyerr(py: Python, pyerr: &PyErr) -> String {
    let value_bound = pyerr.value_bound(py);
    // let type_bound = pyerr.get_type_bound(py);
    if let Ok(s) = value_bound.str() {
        match value_bound.get_type().qualname() {
            Ok(name) => format!("{0} : {1}", name, &s.to_string_lossy()),
            Err(_) => format!("Unknown Error Type : {}", &s.to_string_lossy()),
        }
    } else {
        "<exception str() failed>".into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserPythonCompileMode {
    EvalExpr,
    ExecStmts,
    ExecIndentedStmts,
}

/// The contexts in which you might execute Python on user-generated code or objects
#[derive(Error, Debug)]
pub enum UserPythonExecError {
    // Considered throwing an error here, but I'm not convinced it's common enough to try and detect.
    // If using e.g. template-generated turnip-text it might be useful?
    // It would certainly be unholy, but I don't think this error would make it better.
    // #[error("Found empty eval-brackets, likely not what was intended")]
    // EmptyEvalBrackets { code_ctx: ParseContext },
    /// Compiling user-supplied code
    #[error("Error when compiling code from eval-brackets: {err}")]
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
        code: ParseContext,
        obj: PyObject,
        err: PyErr,
    },
}

#[derive(Error, Debug)]
pub enum TurnipTextContextlessError {
    #[error("Interpreter Error: {0}")]
    Interp(#[from] Box<InterpError>),
    #[error("Error when executing user-generated Python")]
    UserPython(#[from] Box<UserPythonExecError>),
    #[error("Internal Python Error: {0}")]
    InternalPython(String), // TODO take PyErr
}
impl From<InterpError> for TurnipTextContextlessError {
    fn from(value: InterpError) -> Self {
        Self::Interp(Box::new(value))
    }
}
impl From<UserPythonExecError> for TurnipTextContextlessError {
    fn from(value: UserPythonExecError) -> Self {
        Self::UserPython(Box::new(value))
    }
}
impl From<(Python<'_>, PyErr)> for TurnipTextContextlessError {
    fn from(value: (Python, PyErr)) -> Self {
        Self::InternalPython(stringify_pyerr(value.0, &value.1))
    }
}

pub type TurnipTextContextlessResult<T> = Result<T, TurnipTextContextlessError>;

#[derive(Error, Debug)]
pub enum TurnipTextError {
    #[error("Found a null byte '\\0' in source '{source_name}', which isn't allowed. This source is probably corrupted, not a text file, or was read with the wrong encoding.")]
    NullByteFoundInSource { source_name: String },
    #[error("Interpreter Error: {1}")]
    Interp(Vec<ParsingFile>, Box<InterpError>),
    #[error("Error when executing user-generated Python")]
    UserPython(Vec<ParsingFile>, Box<UserPythonExecError>),
    #[error("Internal Python Error: {0}")]
    InternalPython(String),
}
impl From<(Vec<ParsingFile>, TurnipTextContextlessError)> for TurnipTextError {
    fn from(value: (Vec<ParsingFile>, TurnipTextContextlessError)) -> Self {
        match value.1 {
            TurnipTextContextlessError::Interp(err) => Self::Interp(value.0, err),
            TurnipTextContextlessError::UserPython(err) => Self::UserPython(value.0, err),
            TurnipTextContextlessError::InternalPython(err) => Self::InternalPython(err),
        }
    }
}
impl TurnipTextError {
    pub fn display_cli_feedback(&self) {
        let dl = DisplayList::from(self.snippet());
        eprintln!("{}", dl);
    }
}

pub type TurnipTextResult<T> = Result<T, TurnipTextError>;
