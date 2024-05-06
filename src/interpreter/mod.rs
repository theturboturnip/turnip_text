use pyo3::prelude::*;
use pyo3::{types::PyDict, Py, Python};
use std::fmt::Debug;
use std::num::NonZeroUsize;

use crate::python::interop::{Document, TurnipTextSource};
use crate::util::ParseSpan;

use self::state_machines::ProcessorStacks;

pub mod error;
pub(crate) mod lexer; // pub(crate) for testing
mod state_machines;

use error::{TTErrorWithContext, TTResultWithContext};
use lexer::{lex, LexedStrIterator};

pub type UserPythonEnv<'a> = &'a Bound<'a, PyDict>;

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
    pub fn new(file_idx: usize, name: String, contents: String) -> TTResultWithContext<Self> {
        // Can't use map_err here because the closure can't move out of name if we use name later
        let token_stream = match lex(file_idx, &contents) {
            Ok(ts) => ts,
            Err(_) => return Err(TTErrorWithContext::NullByteFoundInSource { source_name: name }),
        };
        Ok(Self {
            name,
            contents,
            token_stream,
        })
    }

    pub fn name<'a>(&'a self) -> &'a str {
        &self.name
    }

    pub fn contents<'a>(&'a self) -> &'a str {
        &self.contents
    }
}

pub enum FileEvent {
    FileInserted {
        emitted_by_code: ParseSpan,
        name: String,
        contents: String,
    },
    FileEnded,
}

pub struct RecursionConfig {
    /// Print a warning to stderr on possible recursion
    pub recursion_warning: bool,
    /// Hard limit on the maximum depth of included source files
    pub max_file_depth: Option<NonZeroUsize>,
}

pub struct TurnipTextParser {
    /// Configuration for recursion protection
    recursion_config: RecursionConfig,
    /// The stack of currently parsed file (spawned from, indices). `spawned from` is None for the first file, Some for all others
    file_stack: Vec<(Option<ParseSpan>, usize)>,
    files: Vec<ParsingFile>,
    builders: ProcessorStacks,
}
impl TurnipTextParser {
    pub fn oneshot_parse(
        py: Python,
        py_env: UserPythonEnv,
        file: TurnipTextSource,
        recursion_config: RecursionConfig,
    ) -> TTResultWithContext<Py<Document>> {
        let parser = Self::new(py, file.name, file.contents, recursion_config)?;
        parser.parse(py, py_env)
    }

    pub fn new(
        py: Python,
        file_name: String,
        file_contents: String,
        recursion_config: RecursionConfig,
    ) -> TTResultWithContext<Self> {
        let file = ParsingFile::new(0, file_name, file_contents)?;
        let files = vec![file];
        let builders = match ProcessorStacks::new(py) {
            Ok(b) => b,
            Err(err) => return Err((files, err).into()),
        };
        Ok(Self {
            recursion_config,
            file_stack: vec![(None, 0)],
            files,
            builders,
        })
    }
    pub fn parse(mut self, py: Python, py_env: UserPythonEnv) -> TTResultWithContext<Py<Document>> {
        // Call process_tokens until it breaks out returning FileInserted or FileEnded.
        // FileEnded will be returned exactly once more than FileInserted - FileInserted is only returned for subfiles, FileEnded is returned for all subfiles AND the initial file.
        // We handle this because the file stack, Vec<ParsingFile>, and interpreter each have one file's worth of content pushed in initially.
        loop {
            let action = {
                let file_idx = match self.file_stack.last_mut() {
                    None => break,
                    Some((_, file_idx)) => file_idx,
                };
                let file = &mut self.files[*file_idx];
                self.builders.top_stack().process_tokens(
                    py,
                    py_env,
                    &mut file.token_stream,
                    &file.contents,
                )
            };
            let action = match action {
                Ok(action) => action,
                Err(err) => return Err((self.files, err).into()),
            };
            match action {
                FileEvent::FileInserted {
                    emitted_by_code,
                    name,
                    contents,
                } => {
                    let file_idx = self.files.len();

                    // Brittle warning for recursion
                    if self.recursion_config.recursion_warning {
                        for (_, other_file_idx) in &self.file_stack {
                            if self.files[*other_file_idx].name == name {
                                eprintln!(
                                "turnip_text warning: likely recursion in source named '{name}'"
                            )
                            }
                        }
                    }

                    self.files.push(ParsingFile::new(file_idx, name, contents)?);
                    self.file_stack.push((Some(emitted_by_code), file_idx));
                    self.builders.push_subfile();

                    if let Some(limit) = self.recursion_config.max_file_depth {
                        if self.file_stack.len() > limit.into() {
                            return Err(TTErrorWithContext::FileStackExceededLimit {
                                files: self.files,
                                limit: limit.into(),
                            });
                        }
                    }
                }
                FileEvent::FileEnded => {
                    let (emitted_by, _) = self
                        .file_stack
                        .pop()
                        .expect("We just handled tokens from a file, there must be one");
                    self.builders.pop_subfile(emitted_by)
                }
            };
        }

        self.builders
            .finalize(py)
            .map_err(|err| (self.files, err).into())
    }
}
