use annotate_snippets::display_list::DisplayList;
use pyo3::{PyErr, Python};
use thiserror::Error;

use crate::{interpreter::ParsingFile, python::error::set_cause_and_context};

use self::{syntax::TTSyntaxError, user_python::TTUserPythonError};

mod display;
pub mod syntax;
pub mod user_python;

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
    #[error("Internal Python Error")]
    InternalPython(#[from] PyErr),
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

/// An Error
#[derive(Error, Debug)]
pub enum TTErrorWithContext {
    #[error("Found a null byte '\\0' in source '{source_name}', which isn't allowed. This source is probably corrupted, not a text file, or was read with the wrong encoding.")]
    NullByteFoundInSource { source_name: String },
    #[error("Interpreter Error: {1}")]
    Syntax(Vec<ParsingFile>, Box<TTSyntaxError>),
    #[error("Error when executing user-generated Python")]
    UserPython(Vec<ParsingFile>, Box<TTUserPythonError>),
    #[error("Internal Python Error")]
    InternalPython(#[from] PyErr),
}
impl From<(Vec<ParsingFile>, TTError)> for TTErrorWithContext {
    fn from(value: (Vec<ParsingFile>, TTError)) -> Self {
        match value.1 {
            TTError::Syntax(err) => Self::Syntax(value.0, err),
            TTError::UserPython(err) => Self::UserPython(value.0, err),
            TTError::InternalPython(err) => Self::InternalPython(err),
        }
    }
}
impl TTErrorWithContext {
    pub fn display_cli_feedback(&self) {
        let dl = DisplayList::from(self.snippet());
        eprintln!("{}", dl);
    }

    pub fn to_pyerr(self, py: Python) -> PyErr {
        let msg = format!("{}", DisplayList::from(self.snippet()));
        let mut basic_err = crate::python::interop::TurnipTextError::new_err(msg);

        match self {
            // If the error wasn't related to an actual PyErr, just throw the exception as-is
            TTErrorWithContext::NullByteFoundInSource { .. } | TTErrorWithContext::Syntax(_, _) => {
            }
            // If it *was* related to an actual PyErr, set __cause__ and __context__ to point to that error.
            TTErrorWithContext::UserPython(_, user_python_err) => match *user_python_err {
                // Coercion doesn't have an actual PyError associated with it
                TTUserPythonError::CoercingNonBuilderEvalBracket { .. } => {}
                TTUserPythonError::CompilingEvalBrackets { err, .. }
                | TTUserPythonError::RunningEvalBrackets { err, .. }
                | TTUserPythonError::CoercingBlockScopeBuilder { err, .. }
                | TTUserPythonError::CoercingInlineScopeBuilder { err, .. }
                | TTUserPythonError::CoercingRawScopeBuilder { err, .. } => {
                    set_cause_and_context(py, &mut basic_err, err)
                }
            },
            TTErrorWithContext::InternalPython(err) => {
                set_cause_and_context(py, &mut basic_err, err)
            }
        }

        basic_err
    }
}

pub type TTResultWithContext<T> = Result<T, TTErrorWithContext>;
