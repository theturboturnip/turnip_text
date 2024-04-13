use std::fmt::Debug;

use pyo3::{types::PyDict, Py, Python};

use crate::{
    error::{stringify_pyerr, TurnipTextError, TurnipTextResult},
    interpreter::{python::interop::DocSegment, InterpreterFileAction},
    lexer::{lex, LexedStrIterator},
    util::ParseSpan,
};

pub struct ParsingFile {
    name: String,
    contents: String,
    token_stream: LexedStrIterator,
}
impl Debug for ParsingFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsingFile")
            .field("name", &self.name)
            .field("contents", &self.contents)
            .field("token_stream", &"...".to_string())
            .finish()
    }
}
impl ParsingFile {
    pub fn new(file_idx: usize, name: String, contents: String) -> Self {
        Self {
            name,
            token_stream: lex(file_idx, &contents),
            contents,
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
    // The stack of currently parsed file (spawned from, indices). `spawned from` is None for the first file, Some for all others
    file_stack: Vec<(Option<ParseSpan>, usize)>,
    files: Vec<ParsingFile>,
    interp: crate::interpreter::next::Interpreter,
}
impl TurnipTextParser {
    pub fn new(py: Python, file_name: String, file_contents: String) -> TurnipTextResult<Self> {
        let file = ParsingFile::new(0, file_name, file_contents);
        let files = vec![file];
        let mut interp = crate::interpreter::next::Interpreter::new(py)
            .map_err(|pyerr| TurnipTextError::InternalPython(stringify_pyerr(py, &pyerr)))?;
        Ok(Self {
            file_stack: vec![(None, 0)],
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
                    Some((_, file_idx)) => file_idx,
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
                InterpreterFileAction::FileInserted {
                    emitted_by,
                    name,
                    contents,
                } => {
                    let file_idx = self.files.len();
                    self.files.push(ParsingFile::new(file_idx, name, contents));
                    self.file_stack.push((Some(emitted_by), file_idx));
                    self.interp.push_subfile();
                }
                InterpreterFileAction::FileEnded => {
                    let (emitted_by, _) = self
                        .file_stack
                        .pop()
                        .expect("We just handled tokens from a file, there must be one");
                    match self.interp.pop_subfile(py, py_env, emitted_by) {
                        Ok(()) => {}
                        Err(err) => return Err((self.files, err).into()),
                    };
                }
            };
        }

        self.interp
            .finalize(py, py_env)
            .map_err(|err| (self.files, err).into())
    }
}
