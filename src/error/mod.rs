use annotate_snippets::display_list::DisplayList;
use pyo3::{PyErr, PyObject, Python};
use thiserror::Error;

use crate::{interpreter::ParsingFile, util::ParseContext};

use self::interp::InterpError;

mod display;
pub mod interp;

pub fn stringify_pyerr(py: Python, pyerr: &PyErr) -> String {
    let value = pyerr.value(py);
    let type_name = match value.get_type().name() {
        Ok(name) => name,
        Err(_) => "Unknown Type",
    };
    if let Ok(s) = value.str() {
        format!("{0} : {1}", type_name, &s.to_string_lossy())
    } else {
        "<exception str() failed>".into()
    }
}

/// The contexts in which you might execute Python on user-generated code or objects
#[derive(Error, Debug)]
pub enum UserPythonExecError {
    /// Directly running user-supplied code
    #[error("Error when executing code from eval-brackets")]
    RunningEvalBrackets { code: ParseContext, err: PyErr },
    /// Ran user code from an eval-bracket which didn't have an argument attached,
    /// failed to coerce the code result to Block, Inline, Header, or TurnipTextSource
    #[error("Successfully evaluated eval-brackets, but the output was not None, a TurnipTextSource, a Header, a Block, or coercible to Inline")]
    CoercingNonBuilderEvalBracket { code: ParseContext, obj: PyObject },
    /// Ran user code from an eval-bracket which was followed by a block scope argument,
    /// but failed to coerce the code result to BlockScopeBuilder
    #[error("Successfully evaluated eval-brackets, constructed a block-scope to provide to a builder, but raised an error when building the inline scope")]
    CoercingBlockScopeBuilder {
        code: ParseContext,
        obj: PyObject,
        err: PyErr,
    },
    /// Ran user code from an eval-bracket which was followed by an inline scope argument,
    /// but failed to coerce the code result to InlineScopeBuilder
    #[error("Successfully evaluated eval-brackets, constructed an inline-scope to provide to a builder, but raised an error when building the inline scope")]
    CoercingInlineScopeBuilder {
        code: ParseContext,
        obj: PyObject,
        err: PyErr,
    },
    /// Ran user code from an eval-bracket which was followed by a raw scope argument,
    /// but failed to coerce the code result to RawScopeBuilder
    #[error("Successfully evaluated eval-brackets, constructed a raw-scope to provide to a builder, but the eval-bracket output was not a RawScopeBuilder")]
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
