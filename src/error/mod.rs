use annotate_snippets::display_list::DisplayList;
use pyo3::{PyErr, Python};
use thiserror::Error;

use crate::{interpreter::InterpError, interpreter::ParsingFile, lexer::LexError};

mod display;

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

#[derive(Error, Debug)]
pub enum TurnipTextContextlessError {
    #[error("Syntax Error: {1}")]
    Lex(usize, LexError),
    #[error("Interpreter Error: {0}")]
    Interp(#[from] Box<InterpError>),
    #[error("Internal Error: {0}")]
    Internal(String),
    #[error("Internal Python Error: {0}")]
    InternalPython(String),
}
impl From<InterpError> for TurnipTextContextlessError {
    fn from(value: InterpError) -> Self {
        Self::Interp(Box::new(value))
    }
}
impl From<(usize, LexError)> for TurnipTextContextlessError {
    fn from(value: (usize, LexError)) -> Self {
        Self::Lex(value.0, value.1)
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
    #[error("Syntax Error {2}")]
    Lex(Vec<ParsingFile>, usize, LexError),
    #[error("Interpreter Error {1}")]
    Interp(Vec<ParsingFile>, Box<InterpError>),
    #[error("Internal Error {0}")]
    Internal(String),
    #[error("Internal Python Error {0}")]
    InternalPython(String),
}
impl From<(Vec<ParsingFile>, TurnipTextContextlessError)> for TurnipTextError {
    fn from(value: (Vec<ParsingFile>, TurnipTextContextlessError)) -> Self {
        match value.1 {
            TurnipTextContextlessError::Lex(file_idx, err) => Self::Lex(value.0, file_idx, err),
            TurnipTextContextlessError::Interp(err) => Self::Interp(value.0, err),
            TurnipTextContextlessError::Internal(err) => Self::Internal(err),
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
