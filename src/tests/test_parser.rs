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

#[test]
pub fn test_basic_text() {
    expect_parse(
        r#"Lorem Ipsum is simply dummy text of the printing and typesetting industry.
Lorem Ipsum has been the industry's standard dummy text ever since the 1500s, when an unknown printer took a galley of type and scrambled it to make a type specimen book.
It has survived not only five centuries, but also the leap into electronic typesetting, remaining essentially unchanged.
It was popularised in the 1960s with the release of Letraset sheets containing Lorem Ipsum passages, and more recently with desktop publishing software like Aldus PageMaker including versions of Lorem Ipsum.
"#,
        Ok(
            test_doc(vec![
                TestBlock::Paragraph(vec![
                    test_sentence("Lorem Ipsum is simply dummy text of the printing and typesetting industry."),
                    test_sentence("Lorem Ipsum has been the industry's standard dummy text ever since the 1500s, when an unknown printer took a galley of type and scrambled it to make a type specimen book."),
                    test_sentence("It has survived not only five centuries, but also the leap into electronic typesetting, remaining essentially unchanged."),
                    test_sentence("It was popularised in the 1960s with the release of Letraset sheets containing Lorem Ipsum passages, and more recently with desktop publishing software like Aldus PageMaker including versions of Lorem Ipsum."),
                ])
            ])
        )
    )
}

#[test]
pub fn test_inline_code() {
    expect_parse(
        r#"Number of values in (1,2,3): [len((1,2,3))]"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Number of values in (1,2,3): "),
            test_text("3"),
        ]])])),
    )
}

#[test]
pub fn test_inline_code_with_extra_delimiter() {
    expect_parse(
        r#"Number of values in (1,2,3): [[ len((1,2,3)) ]]"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Number of values in (1,2,3): "),
            test_text("3"),
        ]])])),
    )
}

#[test]
pub fn test_inline_code_with_long_extra_delimiter() {
    expect_parse(
        r#"Number of values in (1,2,3): [[[[[ len((1,2,3)) ]]]]]"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Number of values in (1,2,3): "),
            test_text("3"),
        ]])])),
    )
}

#[test]
pub fn test_inline_code_with_escaped_extra_delimiter() {
    expect_parse(
        r#"Number of values in (1,2,3): \[[ len((1,2,3)) ]\]"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Number of values in (1,2,3): ["),
            test_text("3"),
            test_text("]"),
        ]])])),
    )
}

#[test]
pub fn test_inline_escaped_code_with_escaped_extra_delimiter() {
    expect_parse(
        r#"Number of values in (1,2,3): \[\[ len((1,2,3)) \]\]"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            r#"Number of values in (1,2,3): [[ len((1,2,3)) ]]"#,
        )])])),
    )
}

#[test]
pub fn test_inline_list_with_extra_delimiter() {
    expect_parse(
        r#"Number of values in (1,2,3): [[ len([1,2,3]) ]]"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Number of values in (1,2,3): "),
            test_text("3"),
        ]])])),
    )
}

#[test]
pub fn test_block_scope() {
    expect_parse(
        r#"Outside the scope

{
Inside the scope

Second paragraph inside the scope
}"#,
        Ok(test_doc(vec![
            TestBlock::Paragraph(vec![test_sentence("Outside the scope")]),
            TestBlock::BlockScope {
                owner: None,
                contents: vec![
                    TestBlock::Paragraph(vec![test_sentence("Inside the scope")]),
                    TestBlock::Paragraph(vec![test_sentence("Second paragraph inside the scope")]),
                ],
            },
        ])),
    )
}

#[test]
pub fn test_raw_scope() {
    expect_parse(
        "#{It's f&%#ing raw}#",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::RawText {
                owner: None,
                contents: "It's f&%#ing raw".into(),
            },
        ]])])),
    )
}

#[test]
pub fn test_inline_scope() {
    expect_parse(
        r#"Outside the scope {inside the scope}"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Outside the scope "),
            TestInline::InlineScope {
                owner: None,
                contents: vec![test_text("inside the scope")],
            },
        ]])])),
    )
}

#[test]
pub fn test_inline_escaped_scope() {
    expect_parse(
        r#"Outside the scope \{not inside a scope\}"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "Outside the scope {not inside a scope}",
        )])])),
    )
}

#[test]
pub fn test_raw_scope_newlines() {
    expect_parse(
        "Outside the scope #{\ninside the raw scope\n}#",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Outside the scope "),
            test_raw_text(None, "\ninside the raw scope\n"),
        ]])])),
    )
}

/// newlines are converted to \n in all cases in the second tokenization phase, for convenience
#[test]
pub fn test_raw_scope_crlf_newlines() {
    expect_parse(
        "Outside the scope #{\r\ninside the raw scope\r\n}#",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Outside the scope "),
            test_raw_text(None, "\ninside the raw scope\n"),
        ]])])),
    )
}

#[test]
pub fn test_inline_raw_scope() {
    expect_parse(
        r#"Outside the scope #{inside the raw scope}#"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Outside the scope "),
            test_raw_text(None, "inside the raw scope"),
        ]])])),
    )
}

#[test]
pub fn test_owned_block_scope() {
    expect_parse(
        r#"[TEST_BLOCK_OWNER]{
It was the best of the times, it was the blurst of times
}
"#,
        Ok(test_doc(vec![TestBlock::BlockScope {
            owner: Some("TEST_BLOCK_OWNER".into()),
            contents: vec![TestBlock::Paragraph(vec![test_sentence(
                "It was the best of the times, it was the blurst of times",
            )])],
        }])),
    )
}

#[test]
pub fn test_owned_block_scope_with_non_block_owner() {
    expect_parse(
        r#"[None]{
It was the best of the times, it was the blurst of times
}
"#,
        Err(TestInterpError::PythonErr {
            pyerr: "TypeError : Expected object fitting typeclass BlockScopeOwner, didn't get it"
                .into(),
            code_span: TestParserSpan {
                start: (1, 1),
                end: (2, 1),
            },
        }),
    )
}

#[test]
pub fn test_owned_inline_scope() {
    expect_parse(
        r"[TEST_INLINE_OWNER]{special text}",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::InlineScope {
                owner: Some("TEST_INLINE_OWNER".into()),
                contents: vec![test_text("special text")],
            },
        ]])])),
    )
}

#[test]
pub fn test_owned_inline_scope_with_non_inline_owner() {
    expect_parse(
        r"[None]{special text}",
        Err(TestInterpError::PythonErr {
            pyerr: "TypeError : Expected object fitting typeclass InlineScopeOwner, didn't get it"
                .into(),
            code_span: TestParserSpan {
                start: (1, 1),
                end: (1, 8),
            },
        }),
    )
}

#[test]
pub fn test_owned_inline_raw_scope_with_newline() {
    expect_parse(
        r#"[TEST_RAW_OWNER]#{
import os
}#"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::RawText {
                owner: Some("TEST_RAW_OWNER".into()),
                contents: r#"
import os
"#
                .into(),
            },
        ]])])),
    )
}

#[test]
pub fn test_owned_inline_raw_scope_with_non_raw_owner() {
    expect_parse(
        r#"[None]#{
import os
}#"#,
        Err(TestInterpError::PythonErr {
            pyerr: "TypeError : Expected object fitting typeclass RawScopeOwner, didn't get it"
                .into(),
            code_span: TestParserSpan {
                start: (1, 1),
                end: (1, 9),
            },
        }),
    )
}

#[test]
pub fn test_inline_raw_escaped_scope() {
    expect_parse(
        r#"Outside the scope \#\{not inside a scope\}"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "Outside the scope #{not inside a scope}",
        )])])),
    )
}

#[test]
pub fn test_plain_hashes() {
    expect_parse(
        r#"This has a string of ####### hashes in the middle"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("This has a string of "), // The first hash in the chain starts a comment!
        ])])),
    )
}

#[test]
pub fn test_comments() {
    expect_parse(
        r#"It was the best of times, # but...
it was the blurst of times"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("It was the best of times, "),
            test_sentence("it was the blurst of times"),
        ])])),
    )
}

#[test]
pub fn test_special_with_escaped_backslash() {
    expect_parse(
        r#"About to see a backslash! \\[None]"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text(r#"About to see a backslash! \"#),
            test_text("None"),
        ]])])),
    )
}

#[test]
pub fn test_escaped_special_with_escaped_backslash() {
    expect_parse(
        r#"About to see a backslash and square brace! \\\[ that didn't open code!"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            r#"About to see a backslash and square brace! \[ that didn't open code!"#,
        )])])),
    )
}

#[test]
pub fn test_escaped_notspecial() {
    expect_parse(
        r#"\a"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            r#"\a"#,
        )])])),
    )
}

#[test]
pub fn test_escaped_newline() {
    expect_parse(
        r#"escaped \
newline"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "escaped newline",
        )])])),
    )
}

#[test]
pub fn test_newline_in_code() {
    expect_parse(
        "[len((1,\r\n2))]",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "2",
        )])])),
    )
}
#[test]
pub fn test_code_close_in_text() {
    expect_parse(
        "not code ] but closed code",
        Err(TestInterpError::CodeCloseOutsideCode(TestParserSpan {
            start: (1, 10),
            end: (1, 11),
        })),
    )
}
#[test]
pub fn test_scope_close_outside_scope() {
    expect_parse(
        "not in a scope } but closed scope",
        Err(TestInterpError::ScopeCloseOutsideScope(TestParserSpan {
            start: (1, 16),
            end: (1, 17),
        })),
    )
}
#[test]
pub fn test_mismatching_raw_scope_close() {
    expect_parse(
        "##{ text in a scope with a }#",
        Err(TestInterpError::EndedInsideRawScope {
            raw_scope_start: TestParserSpan {
                start: (1, 1),
                end: (1, 4),
            },
        }),
    )
}
#[test]
pub fn test_ended_inside_code() {
    expect_parse(
        "text [code",
        Err(TestInterpError::EndedInsideCode {
            code_start: TestParserSpan {
                start: (1, 6),
                end: (1, 7),
            },
        }),
    )
}
#[test]
pub fn test_ended_inside_raw_scope() {
    expect_parse(
        "text #{raw",
        Err(TestInterpError::EndedInsideRawScope {
            raw_scope_start: TestParserSpan {
                start: (1, 6),
                end: (1, 8),
            },
        }),
    )
}
#[test]
pub fn test_ended_inside_scope() {
    expect_parse(
        "text {scope",
        Err(TestInterpError::SentenceBreakInInlineScope {
            scope_start: TestParserSpan {
                start: (1, 6),
                end: (1, 7),
            },
        }),
    )
}

#[test]
pub fn test_block_scope_vs_inline_scope() {
    expect_parse(
        r#"{
block scope
}{inline scope}"#,
        Ok(test_doc(vec![
            TestBlock::BlockScope {
                owner: None,
                contents: vec![TestBlock::Paragraph(vec![test_sentence("block scope")])],
            },
            TestBlock::Paragraph(vec![vec![TestInline::InlineScope {
                owner: None,
                contents: vec![test_text("inline scope")],
            }]]),
        ])),
    )
}

#[test]
pub fn test_strip_leading_whitespace() {
    expect_parse(
        r#"
        Boy I sure hope this isn't indented!
        It would be bad!
        The test would be broken!"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("Boy I sure hope this isn't indented!"),
            test_sentence("It would be bad!"),
            test_sentence("The test would be broken!"),
        ])])),
    )
}

#[test]
pub fn test_strip_trailing_whitespace() {
    expect_parse(
        concat!(
            r#"No whitespace allowed after this! 
"#,
            r#"I mean it!                        "#
        ),
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("No whitespace allowed after this!"),
            test_sentence("I mean it!"),
        ])])),
    )
}

#[test]
pub fn test_strip_trailing_whitespace_before_comment() {
    expect_parse(
        concat!(
            r#"No whitespace allowed after this!   # commented text doesn't prevent that 
"#,
            r#"I mean it!                          # it really doesn't! "#
        ),
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("No whitespace allowed after this!"),
            test_sentence("I mean it!"),
        ])])),
    )
}

#[test]
pub fn test_not_strip_trailing_whitespace_before_escaped_newline() {
    expect_parse(
        r#"
Whitespace is allowed after this \
because you may need it to split up words in sentences."#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("Whitespace is allowed after this because you may need it to split up words in sentences."),
        ])])),
    )
}