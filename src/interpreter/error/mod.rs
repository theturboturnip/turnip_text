use std::fmt::Display;

use pyo3::{PyErr, Python};
use thiserror::Error;

use crate::{interpreter::ParsingFile, python::error::set_cause_and_context};

use self::{syntax::TTSyntaxError, user_python::TTUserPythonError};

mod display;
use display::detailed_message_of;

pub mod syntax;
pub mod user_python;

/// Implemented only by PyResult<T>
pub trait HandleInternalPyErr {
    type Out;
    /// panic if the result is an err, displaying the error and the provided context.
    /// PyO3 will hopefully unwind the panic and show line numbers.
    fn expect_pyok(self, context: impl Display) -> Self::Out;
}
impl<T> HandleInternalPyErr for Result<T, PyErr> {
    type Out = T;

    fn expect_pyok(self, context: impl Display) -> T {
        match self {
            Ok(t) => t,
            Err(pyerr) => panic!("Internal turnip_text Python error in {context}: {pyerr}"),
        }
    }
}

/// An Error used in code that doesn't currently have access to
/// the TurnipTextSource files that generated the error.
/// May contain ParseSpans, ParseContexts etc. that indicate "in file 5 character 12 was problematic"
/// but doesn't contain "file 5"'s data.
///
/// Converted to [TTErrorWithContext] at the boundary of the interpreter.
#[derive(Error, Debug)]
pub enum TTError {
    #[error("Interpreter Error: {0}")]
    Syntax(#[from] Box<TTSyntaxError>),
    #[error("Error when executing user-generated Python")]
    UserPython(#[from] Box<TTUserPythonError>),
}
impl From<TTSyntaxError> for TTError {
    fn from(value: TTSyntaxError) -> Self {
        Self::Syntax(Box::new(value))
    }
}
impl From<TTUserPythonError> for TTError {
    fn from(value: TTUserPythonError) -> Self {
        Self::UserPython(Box::new(value))
    }
}

pub type TTResult<T> = Result<T, TTError>;

/// An Error returned by the toplevel interpreter, with all the context required for error reporting.
#[derive(Error, Debug)]
pub enum TTErrorWithContext {
    #[error("Found a null byte '\\0' in source '{source_name}', which isn't allowed. This source is probably corrupted, not a text file, or was read with the wrong encoding.")]
    NullByteFoundInSource { source_name: String },
    #[error("The stack of TurnipTextSources exceeded the limit ({limit})")]
    FileStackExceededLimit {
        files: Vec<ParsingFile>,
        /// Implicitly nonzero, NonZeroUsize is difficult to work with
        limit: usize,
    },
    #[error("Interpreter Error: {1}")]
    Syntax(Vec<ParsingFile>, Box<TTSyntaxError>),
    #[error("Error when executing user-generated Python")]
    UserPython(Vec<ParsingFile>, Box<TTUserPythonError>),
}
impl From<(Vec<ParsingFile>, TTError)> for TTErrorWithContext {
    fn from(value: (Vec<ParsingFile>, TTError)) -> Self {
        match value.1 {
            TTError::Syntax(err) => Self::Syntax(value.0, err),
            TTError::UserPython(err) => Self::UserPython(value.0, err),
        }
    }
}
impl TTErrorWithContext {
    pub fn to_pyerr(self, py: Python) -> PyErr {
        let mut basic_err =
            crate::python::interop::TurnipTextError::new_err(detailed_message_of(py, &self));

        match self {
            // If the error wasn't related to an actual PyErr, just throw the exception as-is
            TTErrorWithContext::NullByteFoundInSource { .. }
            | TTErrorWithContext::FileStackExceededLimit { .. }
            | TTErrorWithContext::Syntax(_, _) => {}
            // If it *was* related to an actual PyErr, set __cause__ and __context__ to point to that error.
            TTErrorWithContext::UserPython(_, user_python_err) => match *user_python_err {
                TTUserPythonError::CoercingEvalBracketToElement { err, .. }
                | TTUserPythonError::CompilingEvalBrackets { err, .. }
                | TTUserPythonError::RunningEvalBrackets { err, .. }
                | TTUserPythonError::CoercingEvalBracketToBuilder { err, .. }
                | TTUserPythonError::Building { err, .. }
                | TTUserPythonError::CoercingBuildResultToElement { err, .. } => {
                    set_cause_and_context(py, &mut basic_err, err)
                }
            },
        }

        basic_err
    }
}

pub type TTResultWithContext<T> = Result<T, TTErrorWithContext>;
