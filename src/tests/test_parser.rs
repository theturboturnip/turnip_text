use crate::lexer::{units_to_tokens, Unit};
use crate::tests::test_lexer::TextStream;

use lexer_rs::Lexer;

use crate::python::interop::{
    BlockScope, InlineScope, Paragraph, RawText, Sentence, UnescapedText,
};
use crate::python::{interp_data, InterpError, TurnipTextPython};
use crate::util::ParseSpan;

// Create a static Python instance
use once_cell::sync::Lazy;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::panic;
use std::sync::Mutex;

pub static TTPYTHON: Lazy<Mutex<TurnipTextPython>> =
    Lazy::new(|| Mutex::new(TurnipTextPython::new()));

/// A type mimicking [ParserSpan] for test purposes
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestParserSpan {
    pub start: (usize, usize),
    pub end: (usize, usize),
}
impl From<ParseSpan> for TestParserSpan {
    fn from(p: ParseSpan) -> Self {
        Self {
            start: (p.start.line, p.start.column),
            end: (p.end.line, p.end.column),
        }
    }
}

/// A type mimicking [InterpError] for test purposes
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestInterpError {
    CodeCloseOutsideCode(TestParserSpan),
    ScopeCloseOutsideScope(TestParserSpan),
    RawScopeCloseOutsideRawScope(TestParserSpan),
    EndedInsideCode {
        code_start: TestParserSpan,
    },
    EndedInsideRawScope {
        raw_scope_start: TestParserSpan,
    },
    EndedInsideScope {
        scope_start: TestParserSpan,
    },
    BlockScopeOpenedMidPara {
        scope_start: TestParserSpan,
    },
    BlockOwnerCodeMidPara {
        code_span: TestParserSpan,
    },
    SentenceBreakInInlineScope {
        scope_start: TestParserSpan,
    },
    ParaBreakInInlineScope {
        scope_start: TestParserSpan,
    },
    BlockOwnerCodeHasNoScope {
        code_span: TestParserSpan,
    },
    InlineOwnerCodeHasNoScope {
        code_span: TestParserSpan,
    },
    PythonErr {
        pyerr: String,
        code_span: TestParserSpan,
    },
    InternalPythonErr {
        pyerr: String,
    },
    InternalErr(String),
    EscapedNewlineOutsideParagraph {
        newline: TestParserSpan,
    },
}
impl TestInterpError {
    /// Convert [InterpError] to [TestInterpError]
    ///
    /// This is a lossy transformation, ignoring byte offsets in spans, but is good enough for testing
    pub fn from_interp_error(p: InterpError) -> Self {
        match p {
            InterpError::CodeCloseOutsideCode(span) => Self::CodeCloseOutsideCode(span.into()),
            InterpError::ScopeCloseOutsideScope(span) => Self::ScopeCloseOutsideScope(span.into()),
            InterpError::RawScopeCloseOutsideRawScope(span) => {
                Self::RawScopeCloseOutsideRawScope(span.into())
            }
            InterpError::EndedInsideCode { code_start } => Self::EndedInsideCode {
                code_start: code_start.into(),
            },
            InterpError::EndedInsideRawScope { raw_scope_start } => Self::EndedInsideRawScope {
                raw_scope_start: raw_scope_start.into(),
            },
            InterpError::EndedInsideScope { scope_start } => Self::EndedInsideScope {
                scope_start: scope_start.into(),
            },
            InterpError::BlockScopeOpenedMidPara { scope_start } => Self::BlockScopeOpenedMidPara {
                scope_start: scope_start.into(),
            },
            InterpError::BlockOwnerCodeMidPara { code_span } => Self::BlockOwnerCodeMidPara {
                code_span: code_span.into(),
            },
            InterpError::SentenceBreakInInlineScope { scope_start, .. } => {
                Self::SentenceBreakInInlineScope {
                    scope_start: scope_start.into(),
                }
            }
            InterpError::ParaBreakInInlineScope { scope_start, .. } => {
                Self::ParaBreakInInlineScope {
                    scope_start: scope_start.into(),
                }
            }
            InterpError::BlockOwnerCodeHasNoScope { code_span } => Self::BlockOwnerCodeHasNoScope {
                code_span: code_span.into(),
            },
            InterpError::InlineOwnerCodeHasNoScope { code_span } => {
                Self::InlineOwnerCodeHasNoScope {
                    code_span: code_span.into(),
                }
            }
            InterpError::PythonErr { pyerr, code_span } => Self::PythonErr {
                pyerr,
                code_span: code_span.into(),
            },
            InterpError::InternalPythonErr { pyerr } => Self::InternalPythonErr { pyerr },
            InterpError::InternalErr(s) => Self::InternalErr(s),
            InterpError::EscapedNewlineOutsideParagraph { newline } => {
                Self::EscapedNewlineOutsideParagraph {
                    newline: newline.into(),
                }
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TestBlock {
    BlockScope {
        owner: Option<String>,
        contents: Vec<TestBlock>,
    },
    Paragraph(Vec<Vec<TestInline>>),
}
#[derive(Debug, PartialEq, Eq)]
pub enum TestInline {
    InlineScope {
        owner: Option<String>,
        contents: Vec<TestInline>,
    },
    UnescapedText(String),
    RawText {
        owner: Option<String>,
        contents: String,
    },
}
pub fn test_doc(contents: Vec<TestBlock>) -> TestBlock {
    TestBlock::BlockScope {
        owner: None,
        contents,
    }
}
pub fn test_sentence(s: impl Into<String>) -> Vec<TestInline> {
    vec![TestInline::UnescapedText(s.into())]
}
pub fn test_text(s: impl Into<String>) -> TestInline {
    TestInline::UnescapedText(s.into())
}
pub fn test_raw_text(owner: Option<String>, s: impl Into<String>) -> TestInline {
    TestInline::RawText {
        owner,
        contents: s.into(),
    }
}

pub trait PyToTest<T> {
    fn as_test(&self, py: Python) -> T;
}
impl PyToTest<TestBlock> for PyAny {
    fn as_test(&self, py: Python) -> TestBlock {
        if let Ok(block) = self.extract::<BlockScope>() {
            TestBlock::BlockScope {
                owner: block.owner.map(|x| x.as_ref(py).to_string()),
                contents: block
                    .children
                    .list(py)
                    .iter()
                    .map(|obj| PyToTest::as_test(obj, py))
                    .collect(),
            }
        } else if let Ok(para) = self.extract::<Paragraph>() {
            TestBlock::Paragraph(
                para.0
                    .list(py)
                    .iter()
                    .map(|obj| PyToTest::as_test(obj, py))
                    .collect(),
            )
        } else {
            panic!("Python BlockNode-like is neither BlockScope or Paragraph")
        }
    }
}
impl PyToTest<Vec<TestInline>> for PyAny {
    fn as_test(&self, py: Python) -> Vec<TestInline> {
        if let Ok(sentence) = self.extract::<Sentence>() {
            sentence
                .0
                .list(py)
                .iter()
                .map(|obj| PyToTest::as_test(obj, py))
                .collect()
        } else {
            panic!("Python Sentence-like is not Sentence")
        }
    }
}
impl PyToTest<TestInline> for PyAny {
    fn as_test(&self, py: Python) -> TestInline {
        if let Ok(inl) = self.extract::<InlineScope>() {
            TestInline::InlineScope {
                owner: inl.owner.map(|x| x.as_ref(py).to_string()),
                contents: inl
                    .children
                    .list(py)
                    .iter()
                    .map(|obj| PyToTest::as_test(obj, py))
                    .collect(),
            }
        } else if let Ok(text) = self.extract::<UnescapedText>() {
            TestInline::UnescapedText(text.0.as_ref(py).to_string())
        } else if let Ok(text) = self.extract::<RawText>() {
            TestInline::RawText {
                owner: text.owner.map(|x| x.as_ref(py).to_string()),
                contents: text.contents.as_ref(py).to_string(),
            }
        } else {
            TestInline::UnescapedText(
                self.str()
                    .expect("Failed to stringify output Python object")
                    .to_string(),
            )
        }
    }
}

/// Generate a set of local Python variables used in each test case
///
/// Provides `TEST_BLOCK_OWNER`, `TEST_INLINE_OWNER`, `TEST_RAW_OWNER` objects
/// that can own block, inline, and raw scopes respectively.
pub fn generate_globals<'interp>(py: Python<'interp>) -> PyResult<&'interp PyDict> {
    let globals = PyDict::new(py);

    py.run(
        r#"
class TestOwner:
    def __init__(self, name):
        self.name = name
    def __str__(self):
        return self.name
    def __call__(self, x):
        return str(x)

TEST_BLOCK_OWNER = TestOwner("TEST_BLOCK_OWNER")
TEST_BLOCK_OWNER.owns_block_scope = True

TEST_INLINE_OWNER = TestOwner("TEST_INLINE_OWNER")
TEST_INLINE_OWNER.owns_inline_scope = True

TEST_RAW_OWNER = TestOwner("TEST_RAW_OWNER")
TEST_RAW_OWNER.owns_raw_scope = True
"#,
        None,
        Some(globals),
    )?;

    Ok(globals)
}

/// Run the lexer and parser on a given piece of text, convert the parsed result to our test versions, and compare with the expected result.
fn expect_parse<'a>(data: &str, expected_parse: Result<TestBlock, TestInterpError>) {
    println!("{:?}", data);

    // First step: lex
    let l = TextStream::new(data);
    let units: Vec<Unit> = l
        .iter(&[Box::new(Unit::parse_special), Box::new(Unit::parse_other)])
        .scan((), |_, x| x.ok())
        .collect();
    let stoks = units_to_tokens(units);

    // Second step: parse
    // Need to do this safely so that we don't panic while the TTPYTHON mutex is taken -
    // that would poison the mutex and break subsequent tests.
    let root: Result<Result<TestBlock, TestInterpError>, _> = {
        // Lock mutex
        let ttpython = TTPYTHON.lock().unwrap();
        // Catch all non-abort panics while running the interpreter
        // and handling the output
        panic::catch_unwind(|| {
            ttpython
                .with_gil(|py| {
                    let globals = generate_globals(py).expect("Couldn't generate globals dict");
                    let root = interp_data(py, globals, data, stoks.into_iter());
                    root.map(|bs| {
                        let bs_obj = bs.to_object(py);
                        let bs: &PyAny = bs_obj.as_ref(py);
                        (bs as &dyn PyToTest<TestBlock>).as_test(py)
                    })
                })
                .map_err(TestInterpError::from_interp_error)
        })
        // Unlock mutex
    };
    // If any of the python-related code tried to panic, re-panic here now the mutex is unlocked
    match root {
        Ok(root) => assert_eq!(root, expected_parse),
        Err(e) => panic!("{:?}", e),
    }
}
