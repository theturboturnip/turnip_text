use std::fmt::Debug;

use pyo3::{types::PyDict, Py, Python};

use crate::{
    error::{stringify_pyerr, TurnipTextError, TurnipTextResult},
    interpreter::{python::interop::DocSegment, Interpreter, InterpreterFileAction},
    lexer::{lex, LexedStrIterator},
    util::ParseSpan,
};

pub struct ParsingFile {
    name: String,
    contents: String,
    token_stream: LexedStrIterator,
    included_from: Option<ParseSpan>,
}
impl Debug for ParsingFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsingFile")
            .field("name", &self.name)
            .field("contents", &self.contents)
            .field("token_stream", &"...".to_string())
            .field("included_from", &self.included_from)
            .finish()
    }
}
impl ParsingFile {
    pub fn new(
        file_idx: usize,
        name: String,
        contents: String,
        included_from: Option<ParseSpan>,
    ) -> Self {
        Self {
            name,
            token_stream: lex(file_idx, &contents),
            contents,
            included_from,
        }
    }

    pub fn name<'a>(&'a self) -> &'a str {
        &self.name
    }

    pub fn contents<'a>(&'a self) -> &'a str {
        &self.contents
    }
}

pub struct TurnipTextParser {
    // The stack of currently parsed file indices
    file_stack: Vec<usize>,
    files: Vec<ParsingFile>,
    interp: Interpreter,
}
impl TurnipTextParser {
    pub fn new(py: Python, file_name: String, file_contents: String) -> TurnipTextResult<Self> {
        let file = ParsingFile::new(0, file_name, file_contents, None);
        let files = vec![file];
        let mut interp = Interpreter::new(py)
            .map_err(|pyerr| TurnipTextError::InternalPython(stringify_pyerr(py, &pyerr)))?;
        interp.push_subfile(); // We start out with one subfile - the file we're initially parsing
        Ok(Self {
            file_stack: vec![0],
            files,
            interp,
        })
    }
    pub fn parse(mut self, py: Python, py_env: &PyDict) -> TurnipTextResult<Py<DocSegment>> {
        // Call handle_tokens until it breaks out returning FileInserted or FileEnded.
        // FileEnded will be returned exactly once more than FileInserted - FileInserted is only returned for subfiles, FileEnded is returned for all subfiles AND the initial file.
        // We handle this because the file stack, Vec<ParsingFile>, and interpreter each have one file's worth of content pushed in initially.
        loop {
            let action = {
                let file_idx = match self.file_stack.last_mut() {
                    None => break,
                    Some(file_idx) => file_idx,
                };
                let file = &mut self.files[*file_idx];
                self.interp.handle_tokens(
                    py,
                    py_env,
                    &mut file.token_stream,
                    *file_idx,
                    &file.contents,
                )
            };
            let action = match action {
                Ok(action) => action,
                Err(err) => return Err((self.files, err).into()),
            };
            match action {
                InterpreterFileAction::FileInserted { name, contents } => {
                    let file_idx = self.files.len();
                    self.files
                        .push(ParsingFile::new(file_idx, name, contents, None));
                    self.file_stack.push(file_idx);
                    self.interp.push_subfile();
                }
                InterpreterFileAction::FileEnded => {
                    match self.interp.pop_subfile(py, py_env) {
                        Ok(()) => {}
                        Err(err) => return Err((self.files, err).into()),
                    };
                    self.file_stack.pop().expect("There must be a file!");
                }
            };
        }

        self.interp
            .finalize(py, py_env)
            .map_err(|err| (self.files, err).into())
    }
}
