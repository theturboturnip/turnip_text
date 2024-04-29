use crate::error::TurnipTextResult;
use crate::interpreter::TurnipTextParser;
use regex::Regex;

use crate::python::prepare_freethreaded_turniptext_python;

use pyo3::prelude::*;

use std::panic;
// We need to initialize Python the first time we test
use std::sync::Once;

use super::helpers::*;
static INIT_PYTHON: Once = Once::new();

/// Run the lexer and parser on a given piece of text, convert the parsed result to our test versions, and compare with the expected result.

pub fn expect_parse_err<'a, T: Into<TestTurnipError<'a>>>(data: &'a str, expected_err: T) {
    expect_parse(data, Err(expected_err.into()))
}

pub fn expect_parse(data: &str, expected_parse: Result<TestDocument, TestTurnipError>) {
    // Make sure Python has been set up
    INIT_PYTHON.call_once(prepare_freethreaded_turniptext_python);

    // Second step: parse
    // Need to do this safely so that we don't panic inside Python::with_gil.
    // I'm not 100% sure but I'm afraid it will poison the GIL and break subsequent tests.
    let root: Result<TurnipTextResult<TestDocument>, _> = {
        // Catch all non-abort panics while running the interpreter
        // and handling the output
        panic::catch_unwind(|| {
            Python::with_gil(|py| {
                let py_env = generate_globals(py).expect("Couldn't generate globals dict");
                let parser = TurnipTextParser::new(py, "<test>".into(), data.into())?;
                let root = parser.parse(py, py_env)?;
                let doc_obj = root.to_object(py);
                let doc: &PyAny = doc_obj.as_ref(py);
                Ok(doc.as_test(py))
            })
        })
        // Unlock mutex
    };
    // If any of the python-related code tried to panic, re-panic here now the mutex is unlocked
    match root {
        Ok(root) => match (&root, &expected_parse) {
            (Ok(doc), Ok(expected_doc)) => assert_eq!(doc, expected_doc),
            (Err(actual_err), Err(expected_err)) => {
                let matches = Python::with_gil(|py| expected_err.matches(py, &actual_err));
                if !matches {
                    panic!(
                        "assertion failed:\nexpected: {expected_err:?}\n  actual: {actual_err:?}"
                    );
                }
            }
            _ => panic!(
                "assertion failed, expected\n\t{expected_parse:?}\ngot\n\t{root:?}\n(mismatching \
                 success)"
            ),
        },
        Err(caught_panic) => panic!("{:?}", caught_panic),
    }
}

/// Unorganized tests of basic functionality
mod basic;

/// Tests that block-level elements (Paragraphs, BlockScope, code-emitted-Blocks, code-emitted-Headers, code-emitted-TurnipTextSources) must be separated by a blank line
mod block_spacing;

/// Tests that the implicit structure mechanism is working correctly, and that headers can/can't be emitted in various places
mod doc_structure;

/// Tests for inserted files: that they function, that they can be created in any kind of block scope, and that errors inside them are handled correctly.
mod inserted_files;

/// Tests for whether {Block,Inline,Raw}ScopeBuilders can emit different kinds of document element, and where those elements can be emitted.
mod code_builder_flexibility;

/// Tests for the code parser where it is initially ambiguous whether code is meant to be an owner or not.
mod code_ambiguity;

/// Tests for the scope parser where it is initially ambiguous whether a scope is inline or block.
mod scope_ambiguity;

/// Tests for situations that are currently allowed but probably shouldn't be.
mod overflexibility;

/// This module checks two kinds of substitution - the only ones performed on text content.
/// - Newlines, whether they be \r\n or \r or \n, are all translated to \n - even in raw scopes
/// - Strings of hyphens
mod substitution;

/// Tests that code is parsed and compiled correctly
mod code;
