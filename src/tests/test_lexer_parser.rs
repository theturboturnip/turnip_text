use crate::lexer::{units_to_tokens, Unit};

use crate::python::interop::{
    BlockScope, InlineScope, Paragraph, RawText, Sentence, UnescapedText,
};
use crate::python::{interp_data, InterpError, TurnipTextPython};
use crate::{
    lexer::{Escapable, LexError, LexPosn, LexToken, TTToken},
    util::ParseSpan,
};
use lexer_rs::{Lexer, LexerOfStr};
use pyo3::prelude::*;

type TextStream<'stream> = LexerOfStr<'stream, LexPosn, LexToken, LexError>;

// Create a static Python instance
use once_cell::sync::Lazy;
use pyo3::types::PyDict;
use std::panic;
use std::sync::Mutex;

static TTPYTHON: Lazy<Mutex<TurnipTextPython>> = Lazy::new(|| Mutex::new(TurnipTextPython::new()));

/// A type mimicking [TTToken] for test purposes
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestTTToken<'a> {
    Newline,
    Escaped(Escapable),
    Backslash,
    CodeOpen(usize),
    CodeClose(usize),
    CodeCloseOwningBlock(usize),
    CodeCloseOwningInline(usize),
    CodeCloseOwningRaw(usize, usize),
    InlineScopeOpen,
    BlockScopeOpen,
    RawScopeOpen(usize),
    RawScopeClose(usize),
    ScopeClose,
    Hashes(usize),
    OtherText(&'a str),
}
impl<'a> TestTTToken<'a> {
    fn from_str_tok(data: &'a str, t: TTToken) -> Self {
        match t {
            TTToken::Newline(_) => Self::Newline,
            TTToken::Escaped(_, escapable) => Self::Escaped(escapable),
            TTToken::Backslash(_) => Self::Backslash,
            TTToken::CodeOpen(_, n) => Self::CodeOpen(n),
            TTToken::CodeClose(_, n) => Self::CodeClose(n),
            TTToken::CodeCloseOwningBlock(_, n) => Self::CodeCloseOwningBlock(n),
            TTToken::CodeCloseOwningInline(_, n) => Self::CodeCloseOwningInline(n),
            TTToken::CodeCloseOwningRaw(_, n_code, n_hashes) => {
                Self::CodeCloseOwningRaw(n_code, n_hashes)
            }
            TTToken::InlineScopeOpen(_) => Self::InlineScopeOpen,
            TTToken::BlockScopeOpen(_) => Self::BlockScopeOpen,
            TTToken::RawScopeOpen(_, n) => Self::RawScopeOpen(n),
            TTToken::RawScopeClose(_, n) => Self::RawScopeClose(n),
            TTToken::ScopeClose(_) => Self::ScopeClose,
            TTToken::Hashes(_, n) => Self::Hashes(n),
            TTToken::OtherText(span) => Self::OtherText(data[span.byte_range()].into()),
        }
    }
}

/// A type mimicking [ParserSpan] for test purposes
#[derive(Debug, Clone, PartialEq, Eq)]
struct TestParserSpan {
    start: (usize, usize),
    end: (usize, usize),
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
enum TestInterpError {
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
    fn from_interp_error(p: InterpError) -> Self {
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
enum TestBlock {
    BlockScope {
        owner: Option<String>,
        contents: Vec<TestBlock>,
    },
    Paragraph(Vec<Vec<TestInline>>),
}
#[derive(Debug, PartialEq, Eq)]
enum TestInline {
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
fn test_doc(contents: Vec<TestBlock>) -> TestBlock {
    TestBlock::BlockScope {
        owner: None,
        contents,
    }
}
fn test_sentence(s: impl Into<String>) -> Vec<TestInline> {
    vec![TestInline::UnescapedText(s.into())]
}
fn test_text(s: impl Into<String>) -> TestInline {
    TestInline::UnescapedText(s.into())
}
fn test_raw_text(owner: Option<String>, s: impl Into<String>) -> TestInline {
    TestInline::RawText {
        owner,
        contents: s.into(),
    }
}

trait PyToTest<T> {
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

fn expect_tokens<'a>(
    data: &str,
    expected_stok_types: Vec<TestTTToken<'a>>,
    expected_parse: Result<TestBlock, TestInterpError>,
) {
    println!("{:?}", data);

    // First step: lex
    let l = TextStream::new(data);
    let units: Vec<Unit> = l
        .iter(&[Box::new(Unit::parse_special), Box::new(Unit::parse_other)])
        .scan((), |_, x| x.ok())
        .collect();
    let stoks = units_to_tokens(units);
    let stok_types: Vec<TestTTToken> = stoks
        .iter()
        .map(|stok| TestTTToken::from_str_tok(data, *stok))
        .collect();

    assert_eq!(stok_types, expected_stok_types);

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
                    let globals = PyDict::new(py);
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

use TestTTToken::*;
#[test]
pub fn test_basic_text() {
    expect_tokens(
        r#"Lorem Ipsum is simply dummy text of the printing and typesetting industry.
Lorem Ipsum has been the industry's standard dummy text ever since the 1500s, when an unknown printer took a galley of type and scrambled it to make a type specimen book.
It has survived not only five centuries, but also the leap into electronic typesetting, remaining essentially unchanged.
It was popularised in the 1960s with the release of Letraset sheets containing Lorem Ipsum passages, and more recently with desktop publishing software like Aldus PageMaker including versions of Lorem Ipsum.
"#,
        vec![
            OtherText("Lorem Ipsum is simply dummy text of the printing and typesetting industry."),
            Newline,
            OtherText("Lorem Ipsum has been the industry's standard dummy text ever since the 1500s, when an unknown printer took a galley of type and scrambled it to make a type specimen book."),
            Newline,
            OtherText("It has survived not only five centuries, but also the leap into electronic typesetting, remaining essentially unchanged."),
            Newline,
            OtherText("It was popularised in the 1960s with the release of Letraset sheets containing Lorem Ipsum passages, and more recently with desktop publishing software like Aldus PageMaker including versions of Lorem Ipsum."),
            Newline,
        ],
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
    expect_tokens(
        r#"Number of values in (1,2,3): [len((1,2,3))]"#,
        vec![
            OtherText("Number of values in (1,2,3): "),
            CodeOpen(1),
            OtherText("len((1,2,3))"),
            CodeClose(1),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Number of values in (1,2,3): "),
            test_text("3"),
        ]])])),
    )
}

#[test]
pub fn test_inline_code_with_extra_delimiter() {
    expect_tokens(
        r#"Number of values in (1,2,3): [[ len((1,2,3)) ]]"#,
        vec![
            OtherText("Number of values in (1,2,3): "),
            CodeOpen(2),
            OtherText(" len((1,2,3)) "),
            CodeClose(2),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Number of values in (1,2,3): "),
            test_text("3"),
        ]])])),
    )
}

#[test]
pub fn test_inline_code_with_long_extra_delimiter() {
    expect_tokens(
        r#"Number of values in (1,2,3): [[[[[ len((1,2,3)) ]]]]]"#,
        vec![
            OtherText("Number of values in (1,2,3): "),
            CodeOpen(5),
            OtherText(" len((1,2,3)) "),
            CodeClose(5),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Number of values in (1,2,3): "),
            test_text("3"),
        ]])])),
    )
}

#[test]
pub fn test_inline_code_with_escaped_extra_delimiter() {
    expect_tokens(
        r#"Number of values in (1,2,3): \[[ len((1,2,3)) ]\]"#,
        vec![
            OtherText("Number of values in (1,2,3): "),
            Escaped(Escapable::SqrOpen),
            CodeOpen(1),
            OtherText(" len((1,2,3)) "),
            CodeClose(1),
            Escaped(Escapable::SqrClose),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Number of values in (1,2,3): ["),
            test_text("3"),
            test_text("]"),
        ]])])),
    )
}

#[test]
pub fn test_inline_escaped_code_with_escaped_extra_delimiter() {
    expect_tokens(
        r#"Number of values in (1,2,3): \[\[ len((1,2,3)) \]\]"#,
        vec![
            OtherText("Number of values in (1,2,3): "),
            Escaped(Escapable::SqrOpen),
            Escaped(Escapable::SqrOpen),
            OtherText(" len((1,2,3)) "),
            Escaped(Escapable::SqrClose),
            Escaped(Escapable::SqrClose),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            r#"Number of values in (1,2,3): [[ len((1,2,3)) ]]"#,
        )])])),
    )
}

#[test]
pub fn test_inline_list_with_extra_delimiter() {
    expect_tokens(
        r#"Number of values in (1,2,3): [[ len([1,2,3]) ]]"#,
        vec![
            OtherText("Number of values in (1,2,3): "),
            CodeOpen(2),
            OtherText(" len("),
            CodeOpen(1),
            OtherText("1,2,3"),
            CodeClose(1),
            OtherText(") "),
            CodeClose(2),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Number of values in (1,2,3): "),
            test_text("3"),
        ]])])),
    )
}

#[test]
pub fn test_block_scope() {
    expect_tokens(
        r#"Outside the scope

{
Inside the scope

Second paragraph inside the scope
}"#,
        vec![
            OtherText("Outside the scope"),
            Newline,
            Newline,
            BlockScopeOpen,
            OtherText("Inside the scope"),
            Newline,
            Newline,
            OtherText("Second paragraph inside the scope"),
            Newline,
            ScopeClose,
        ],
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
    expect_tokens(
        "#{It's f&%#ing raw}#",
        vec![
            RawScopeOpen(1),
            OtherText("It's f&%"),
            Hashes(1),
            OtherText("ing raw"),
            RawScopeClose(1),
        ],
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
    expect_tokens(
        r#"Outside the scope {inside the scope}"#,
        vec![
            OtherText("Outside the scope "),
            InlineScopeOpen,
            OtherText("inside the scope"),
            ScopeClose,
        ],
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
    expect_tokens(
        r#"Outside the scope \{not inside a scope\}"#,
        vec![
            OtherText("Outside the scope "),
            Escaped(Escapable::SqgOpen),
            OtherText("not inside a scope"),
            Escaped(Escapable::SqgClose),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "Outside the scope {not inside a scope}",
        )])])),
    )
}

#[test]
pub fn test_raw_scope_newlines() {
    expect_tokens(
        "Outside the scope #{\ninside the raw scope\n}#",
        vec![
            OtherText("Outside the scope "),
            RawScopeOpen(1),
            Newline,
            OtherText("inside the raw scope"),
            Newline,
            RawScopeClose(1),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Outside the scope "),
            test_raw_text(None, "\ninside the raw scope\n"),
        ]])])),
    )
}

/// newlines are converted to \n in all cases in the second tokenization phase, for convenience
#[test]
pub fn test_raw_scope_crlf_newlines() {
    expect_tokens(
        "Outside the scope #{\r\ninside the raw scope\r\n}#",
        vec![
            OtherText("Outside the scope "),
            RawScopeOpen(1),
            Newline,
            OtherText("inside the raw scope"),
            Newline,
            RawScopeClose(1),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Outside the scope "),
            test_raw_text(None, "\ninside the raw scope\n"),
        ]])])),
    )
}

#[test]
pub fn test_inline_raw_scope() {
    expect_tokens(
        r#"Outside the scope #{inside the raw scope}#"#,
        vec![
            OtherText("Outside the scope "),
            RawScopeOpen(1),
            OtherText("inside the raw scope"),
            RawScopeClose(1),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Outside the scope "),
            test_raw_text(None, "inside the raw scope"),
        ]])])),
    )
}

#[test]
pub fn test_inline_raw_escaped_scope() {
    expect_tokens(
        r#"Outside the scope r\{not inside a scope\}"#,
        vec![
            OtherText("Outside the scope r"),
            Escaped(Escapable::SqgOpen),
            OtherText("not inside a scope"),
            Escaped(Escapable::SqgClose),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "Outside the scope r{not inside a scope}",
        )])])),
    )
}

#[test]
pub fn test_r_without_starting_raw_scope() {
    expect_tokens(
        r#" r doesn't always start a scope "#,
        vec![OtherText(" r doesn't always start a scope ")],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            " r doesn't always start a scope ",
        )])])),
    )
}

#[test]
pub fn test_plain_hashes() {
    expect_tokens(
        r#"This has a string of ####### hashes in the middle"#,
        vec![
            OtherText("This has a string of "),
            Hashes(7),
            OtherText(" hashes in the middle"),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("This has a string of "), // The first hash in the chain starts a comment!
        ])])),
    )
}

#[test]
pub fn test_special_with_escaped_backslash() {
    expect_tokens(
        r#"About to see a backslash! \\[None]"#,
        vec![
            OtherText("About to see a backslash! "),
            Escaped(Escapable::Backslash),
            CodeOpen(1),
            OtherText("None"),
            CodeClose(1),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text(r#"About to see a backslash! \"#),
            test_text("None"),
        ]])])),
    )
}

#[test]
pub fn test_escaped_special_with_escaped_backslash() {
    expect_tokens(
        r#"About to see a backslash and square brace! \\\[ that didn't open code!"#,
        vec![
            OtherText("About to see a backslash and square brace! "),
            Escaped(Escapable::Backslash),
            Escaped(Escapable::SqrOpen),
            OtherText(" that didn't open code!"),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            r#"About to see a backslash and square brace! \[ that didn't open code!"#,
        )])])),
    )
}

#[test]
pub fn test_uneven_code() {
    expect_tokens(
        r#"code with no open]"#,
        vec![OtherText("code with no open"), CodeClose(1)],
        Err(TestInterpError::CodeCloseOutsideCode(TestParserSpan {
            start: (1, 18),
            end: (1, 19),
        })),
    )
}

#[test]
pub fn test_uneven_scope() {
    expect_tokens(
        r#"scope with no open}"#,
        vec![OtherText("scope with no open"), ScopeClose],
        Err(TestInterpError::ScopeCloseOutsideScope(TestParserSpan {
            start: (1, 19),
            end: (1, 20),
        })),
    )
}

#[test]
pub fn test_escaped_notspecial() {
    expect_tokens(
        r#"\a"#,
        vec![Backslash, OtherText("a")],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            r#"\a"#,
        )])])),
    )
}

#[test]
pub fn test_escaped_cr() {
    // '\' + '\r'&
    let s: String = "sentence start, ".to_owned()
        + &['\\', '\r'].iter().collect::<String>()
        + "rest of sentence";
    expect_tokens(
        &s,
        vec![
            OtherText("sentence start, "),
            Escaped(Escapable::Newline),
            OtherText("rest of sentence"),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "sentence start, rest of sentence",
        )])])),
    )
}
#[test]
pub fn test_escaped_lf() {
    // '\' + '\n'
    let s: String = "sentence start, ".to_owned()
        + &['\\', '\n'].iter().collect::<String>()
        + "rest of sentence";
    expect_tokens(
        &s,
        vec![
            OtherText("sentence start, "),
            Escaped(Escapable::Newline),
            OtherText("rest of sentence"),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "sentence start, rest of sentence",
        )])])),
    )
}
#[test]
pub fn test_escaped_crlf() {
    // '\' + '\r' + '\n'
    let s: String = "sentence start, ".to_owned()
        + &['\\', '\r', '\n'].iter().collect::<String>()
        + "rest of sentence";
    expect_tokens(
        &s,
        vec![
            OtherText("sentence start, "),
            Escaped(Escapable::Newline),
            OtherText("rest of sentence"),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "sentence start, rest of sentence",
        )])])),
    )
}

#[test]
pub fn test_cr() {
    // '\r'
    let s: String = ['\r'].iter().collect::<String>() + "content";
    expect_tokens(
        &s,
        vec![Newline, OtherText("content")],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "content",
        )])])),
    )
}
#[test]
pub fn test_lf() {
    // '\n'
    let s: String = ['\n'].iter().collect::<String>() + "content";
    expect_tokens(
        &s,
        vec![Newline, OtherText("content")],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "content",
        )])])),
    )
}
#[test]
pub fn test_crlf() {
    // '\r' + '\n'
    let s: String = ['\r', '\n'].iter().collect::<String>() + "content";
    expect_tokens(
        &s,
        vec![Newline, OtherText("content")],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "content",
        )])])),
    )
}

#[test]
pub fn test_newline_in_code() {
    expect_tokens(
        "[len((1,\r\n2))]",
        vec![
            CodeOpen(1),
            OtherText("len((1,"),
            Newline,
            OtherText("2))"),
            CodeClose(1),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![test_sentence(
            "2",
        )])])),
    )
}
#[test]
pub fn test_code_close_in_text() {
    expect_tokens(
        "not code ] but closed code",
        vec![
            OtherText("not code "),
            CodeClose(1),
            OtherText(" but closed code"),
        ],
        Err(TestInterpError::CodeCloseOutsideCode(TestParserSpan {
            start: (1, 10),
            end: (1, 11),
        })),
    )
}
#[test]
pub fn test_scope_close_outside_scope() {
    expect_tokens(
        "not in a scope } but closed scope",
        vec![
            OtherText("not in a scope "),
            ScopeClose,
            OtherText(" but closed scope"),
        ],
        Err(TestInterpError::ScopeCloseOutsideScope(TestParserSpan {
            start: (1, 16),
            end: (1, 17),
        })),
    )
}
#[test]
pub fn test_mismatching_raw_scope_close() {
    expect_tokens(
        "##{ text in a scope with a }#",
        vec![
            RawScopeOpen(2),
            OtherText(" text in a scope with a "),
            RawScopeClose(1),
        ],
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
    expect_tokens(
        "text [code",
        vec![OtherText("text "), CodeOpen(1), OtherText("code")],
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
    expect_tokens(
        "text #{raw",
        vec![OtherText("text "), RawScopeOpen(1), OtherText("raw")],
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
    expect_tokens(
        "text {scope",
        vec![OtherText("text "), InlineScopeOpen, OtherText("scope")],
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
    expect_tokens(
        r#"{
block scope
}{inline scope}"#,
        vec![
            BlockScopeOpen,
            OtherText("block scope"),
            Newline,
            ScopeClose,
            InlineScopeOpen,
            OtherText("inline scope"),
            ScopeClose,
        ],
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
pub fn test_owned_block_scope() {
    expect_tokens(
        r#"["TestBlockScope"]{
It was the best of the times, it was the blurst of times
}
"#,
        vec![
            CodeOpen(1),
            OtherText(r#""TestBlockScope""#),
            CodeCloseOwningBlock(1),
            OtherText("It was the best of the times, it was the blurst of times"),
            Newline,
            ScopeClose,
            Newline,
        ],
        Ok(test_doc(vec![TestBlock::BlockScope {
            owner: Some("TestBlockScope".into()),
            contents: vec![TestBlock::Paragraph(vec![test_sentence(
                "It was the best of the times, it was the blurst of times",
            )])],
        }])),
    )
}

#[test]
pub fn test_owned_inline_scope() {
    expect_tokens(
        r#"
Some ["TestInlineScope"]{special} text
"#,
        vec![
            Newline,
            OtherText("Some "),
            CodeOpen(1),
            OtherText(r#""TestInlineScope""#),
            CodeCloseOwningInline(1),
            OtherText("special"),
            ScopeClose,
            OtherText(" text"),
            Newline,
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Some "),
            TestInline::InlineScope {
                owner: Some("TestInlineScope".into()),
                contents: vec![test_text("special")],
            },
            test_text(" text"),
        ]])])),
    )
}

#[test]
pub fn test_owned_inline_raw_scope_with_newline() {
    expect_tokens(
        r#"["TestRawScope"]#{
import os
}#"#,
        vec![
            CodeOpen(1),
            OtherText("\"TestRawScope\""),
            CodeCloseOwningRaw(1, 1),
            Newline,
            OtherText("import os"),
            Newline,
            RawScopeClose(1),
        ],
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::RawText {
                owner: Some("TestRawScope".into()),
                contents: r#"
import os
"#
                .into(),
            },
        ]])])),
    )
}
