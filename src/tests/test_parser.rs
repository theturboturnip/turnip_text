use crate::lexer::{units_to_tokens, TTToken, Unit};
use crate::tests::test_lexer::TextStream;

use lexer_rs::Lexer;
use regex::Regex;

use crate::python::interop::{
    BlockScope, InlineScope, Paragraph, RawText, Sentence, UnescapedText,
};
use crate::python::{interp_data, prepare_freethreaded_turniptext_python, InterpError};
use crate::util::ParseSpan;

use pyo3::prelude::*;
use pyo3::types::PyDict;

use std::panic;
// We need to initialize Python the first time we test
use std::sync::Once;
static INIT_PYTHON: Once = Once::new();

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
#[derive(Debug, Clone)]
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
    BlockCodeMidPara {
        code_span: TestParserSpan,
    },
    BlockCodeFromRawScopeMidPara {
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
        pyerr: Regex,
        code_span: TestParserSpan,
    },
    InternalPythonErr {
        pyerr: Regex,
    },
    InternalErr(Regex),
    EscapedNewlineOutsideParagraph {
        newline: TestParserSpan,
    },
}
impl PartialEq<InterpError> for TestInterpError {
    fn eq(&self, other: &InterpError) -> bool {
        match (self, other) {
            (Self::CodeCloseOutsideCode(l0), InterpError::CodeCloseOutsideCode(r0)) => {
                (*l0) == (*r0).into()
            }
            (Self::ScopeCloseOutsideScope(l0), InterpError::ScopeCloseOutsideScope(r0)) => {
                (*l0) == (*r0).into()
            }
            (
                Self::RawScopeCloseOutsideRawScope(l0),
                InterpError::RawScopeCloseOutsideRawScope(r0),
            ) => (*l0) == (*r0).into(),
            (
                Self::EndedInsideCode {
                    code_start: l_code_start,
                },
                InterpError::EndedInsideCode {
                    code_start: r_code_start,
                },
            ) => (*l_code_start) == (*r_code_start).into(),
            (
                Self::EndedInsideRawScope {
                    raw_scope_start: l_raw_scope_start,
                },
                InterpError::EndedInsideRawScope {
                    raw_scope_start: r_raw_scope_start,
                },
            ) => (*l_raw_scope_start) == (*r_raw_scope_start).into(),
            (
                Self::EndedInsideScope {
                    scope_start: l_scope_start,
                },
                InterpError::EndedInsideScope {
                    scope_start: r_scope_start,
                },
            ) => (*l_scope_start) == (*r_scope_start).into(),
            (
                Self::BlockScopeOpenedMidPara {
                    scope_start: l_scope_start,
                },
                InterpError::BlockScopeOpenedMidPara {
                    scope_start: r_scope_start,
                },
            ) => (*l_scope_start) == (*r_scope_start).into(),
            (
                Self::BlockOwnerCodeMidPara {
                    code_span: l_code_span,
                },
                InterpError::BlockOwnerCodeMidPara {
                    code_span: r_code_span,
                },
            ) => (*l_code_span) == (*r_code_span).into(),
            (
                Self::BlockCodeMidPara {
                    code_span: l_code_span,
                },
                InterpError::BlockCodeMidPara {
                    code_span: r_code_span,
                },
            ) => (*l_code_span) == (*r_code_span).into(),
            (
                Self::BlockCodeFromRawScopeMidPara {
                    code_span: l_code_span,
                },
                InterpError::BlockCodeFromRawScopeMidPara {
                    code_span: r_code_span,
                },
            ) => (*l_code_span) == (*r_code_span).into(),
            (
                Self::SentenceBreakInInlineScope {
                    scope_start: l_scope_start,
                },
                InterpError::SentenceBreakInInlineScope {
                    scope_start: r_scope_start,
                },
            ) => (*l_scope_start) == (*r_scope_start).into(),
            (
                Self::ParaBreakInInlineScope {
                    scope_start: l_scope_start,
                },
                InterpError::ParaBreakInInlineScope {
                    scope_start: r_scope_start,
                    ..
                },
            ) => (*l_scope_start) == (*r_scope_start).into(),
            (
                Self::BlockOwnerCodeHasNoScope {
                    code_span: l_code_span,
                },
                InterpError::BlockOwnerCodeHasNoScope {
                    code_span: r_code_span,
                },
            ) => (*l_code_span) == (*r_code_span).into(),
            (
                Self::InlineOwnerCodeHasNoScope {
                    code_span: l_code_span,
                },
                InterpError::InlineOwnerCodeHasNoScope {
                    code_span: r_code_span,
                },
            ) => (*l_code_span) == (*r_code_span).into(),

            (
                Self::PythonErr {
                    pyerr: l_pyerr,
                    code_span: l_code_span,
                },
                InterpError::PythonErr {
                    pyerr: r_pyerr,
                    code_span: r_code_span,
                },
            ) => l_pyerr.is_match(&r_pyerr) && (*l_code_span) == (*r_code_span).into(),
            (
                Self::InternalPythonErr { pyerr: l_pyerr },
                InterpError::InternalPythonErr { pyerr: r_pyerr },
            ) => l_pyerr.is_match(&r_pyerr),

            (Self::InternalErr(l0), InterpError::InternalErr(r0)) => l0.is_match(&r0),

            (
                Self::EscapedNewlineOutsideParagraph { newline: l_newline },
                InterpError::EscapedNewlineOutsideParagraph { newline: r_newline },
            ) => (*l_newline) == (*r_newline).into(),
            _ => false,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TestBlock {
    BlockScope(Vec<TestBlock>),
    Paragraph(Vec<Vec<TestInline>>),

    TestOwnedBlock(Vec<TestBlock>),
}
#[derive(Debug, PartialEq, Eq)]
pub enum TestInline {
    InlineScope(Vec<TestInline>),
    UnescapedText(String),
    RawText(String),

    /// Test-only - a Python object built from an inline scope with test_inline: List[Inline] = the contents of that scope
    TestOwnedInline(Vec<TestInline>),
    /// Test-only - a Python object built from raw text with test_raw_str: str = the raw text
    TestOwnedRaw(String),
}
pub fn test_doc(contents: Vec<TestBlock>) -> TestBlock {
    TestBlock::BlockScope(contents)
}
pub fn test_sentence(s: impl Into<String>) -> Vec<TestInline> {
    vec![TestInline::UnescapedText(s.into())]
}
pub fn test_text(s: impl Into<String>) -> TestInline {
    TestInline::UnescapedText(s.into())
}
pub fn test_raw_text(s: impl Into<String>) -> TestInline {
    TestInline::RawText(s.into())
}

pub trait PyToTest<T> {
    fn as_test(&self, py: Python) -> T;
}
impl PyToTest<TestBlock> for PyAny {
    fn as_test(&self, py: Python) -> TestBlock {
        if let Ok(block) = self.extract::<BlockScope>() {
            TestBlock::BlockScope(
                block
                    .0
                    .list(py)
                    .iter()
                    .map(|obj| PyToTest::as_test(obj, py))
                    .collect(),
            )
        } else if let Ok(para) = self.extract::<Paragraph>() {
            TestBlock::Paragraph(
                para.0
                    .list(py)
                    .iter()
                    .map(|obj| PyToTest::as_test(obj, py))
                    .collect(),
            )
        } else if let Ok(obj) = self.getattr("test_block") {
            TestBlock::TestOwnedBlock(
                obj.extract::<BlockScope>()
                    .unwrap()
                    .0
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
            TestInline::InlineScope(
                inl.0
                    .list(py)
                    .iter()
                    .map(|obj| PyToTest::as_test(obj, py))
                    .collect(),
            )
        } else if let Ok(text) = self.extract::<UnescapedText>() {
            TestInline::UnescapedText(text.0.as_ref(py).to_string())
        } else if let Ok(text) = self.extract::<RawText>() {
            TestInline::RawText(text.0.as_ref(py).to_string())
        } else if let Ok(obj) = self.getattr("test_inline") {
            TestInline::TestOwnedInline(
                obj.extract::<InlineScope>()
                    .unwrap()
                    .0
                    .list(py)
                    .iter()
                    .map(|obj| PyToTest::as_test(obj, py))
                    .collect(),
            )
        } else if let Ok(text) = dbg!(self.getattr("test_raw_str")) {
            TestInline::TestOwnedRaw(text.to_string())
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
/// Provides `TEST_BLOCK_BUILDER`, `TEST_INLINE_BUILDER`, `TEST_RAW_BUILDER` objects
/// that can own block, inline, and raw scopes respectively.
pub fn generate_globals<'interp>(py: Python<'interp>) -> Option<&'interp PyDict> {
    let globals = PyDict::new(py);

    let result = py.run(
        r#"
# The Rust module name is _native, which is included under turnip_text, so Python IDEs don't try to import directly from it.
# This means we use _native instead of turnip_text as the module name here.
from _native import InlineScope, UnescapedText, BlockScope

class FauxBlock:
    is_block = True
    def __init__(self, contents):
        self.test_block = contents

class FauxInline:
    is_inline = True
    def __init__(self, contents):
        self.test_inline = contents

class FauxInlineRaw:
    is_inline = True
    def __init__(self, raw_str):
        self.test_raw_str = str(raw_str)

class TestBuilder:
    def build_from_blocks(self, contents):
        return FauxBlock(contents)
    def build_from_inlines(self, contents):
        return FauxInline(contents)

class TestRawInlineBuilder:
    def build_from_raw(self, raw_str):
        return FauxInlineRaw(raw_str)

TEST_BLOCK = FauxBlock(BlockScope([]))

class TestRawBlockBuilder:
    def build_from_raw(self, raw_str):
        return TEST_BLOCK

class TestBlockSwallower():
    def build_from_blocks(self, contents):
        return None

TEST_BLOCK_BUILDER = TestBuilder()
TEST_INLINE_BUILDER = TestBuilder()
TEST_RAW_INLINE_BUILDER = TestRawInlineBuilder()
TEST_RAW_BLOCK_BUILDER = TestRawBlockBuilder()

TEST_BLOCK_SWALLOWER = TestBlockSwallower()

TEST_PROPERTY = property(lambda x: 5)

"#,
        Some(globals),
        Some(globals),
    );

    match result {
        Err(pyerr) => {
            pyerr.print(py);
            return None;
        }
        Ok(_) => {}
    };

    Some(globals)
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

    expect_parse_tokens(data, stoks, expected_parse)
}

pub fn expect_parse_tokens(
    data: &str,
    stoks: Vec<TTToken>,
    expected_parse: Result<TestBlock, TestInterpError>,
) {
    // Make sure Python has been set up
    INIT_PYTHON.call_once(prepare_freethreaded_turniptext_python);

    // Second step: parse
    // Need to do this safely so that we don't panic inside Python::with_gil.
    // I'm not 100% sure but I'm afraid it will poison the GIL and break subsequent tests.
    let root: Result<Result<TestBlock, InterpError>, _> = {
        // Catch all non-abort panics while running the interpreter
        // and handling the output
        panic::catch_unwind(|| {
            Python::with_gil(|py| {
                let globals = generate_globals(py).expect("Couldn't generate globals dict");
                let root = interp_data(py, globals, data, stoks.into_iter());
                root.map(|bs| {
                    let bs_obj = bs.to_object(py);
                    let bs: &PyAny = bs_obj.as_ref(py);
                    (bs as &dyn PyToTest<TestBlock>).as_test(py)
                })
            })
        })
        // Unlock mutex
    };
    // If any of the python-related code tried to panic, re-panic here now the mutex is unlocked
    match root {
        Ok(root) => {
            if root.is_ok() != expected_parse.is_ok() {
                panic!("assertion failed, expected\n\t{expected_parse:?}\ngot\n\t{root:?}\n(mismatching success)");
            } else {
                match root {
                    Ok(r) => assert_eq!(expected_parse.unwrap(), r),
                    Err(e) => assert_eq!(expected_parse.unwrap_err(), e),
                }
            }
        }
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
            TestBlock::BlockScope(vec![
                TestBlock::Paragraph(vec![test_sentence("Inside the scope")]),
                TestBlock::Paragraph(vec![test_sentence("Second paragraph inside the scope")]),
            ]),
        ])),
    )
}

#[test]
pub fn test_raw_scope() {
    expect_parse(
        "#{It's f&%#ing raw}#",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_raw_text("It's f&%#ing raw"),
        ]])])),
    )
}

#[test]
pub fn test_inline_scope() {
    expect_parse(
        r#"Outside the scope {inside the scope}"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Outside the scope "),
            TestInline::InlineScope(vec![test_text("inside the scope")]),
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
            test_raw_text("\ninside the raw scope\n"),
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
            test_raw_text("\ninside the raw scope\n"),
        ]])])),
    )
}

#[test]
pub fn test_inline_raw_scope() {
    expect_parse(
        r#"Outside the scope #{inside the raw scope}#"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Outside the scope "),
            test_raw_text("inside the raw scope"),
        ]])])),
    )
}

#[test]
pub fn test_owned_block_scope() {
    expect_parse(
        r#"[TEST_BLOCK_BUILDER]{
It was the best of the times, it was the blurst of times
}
"#,
        Ok(test_doc(vec![TestBlock::TestOwnedBlock(vec![
            TestBlock::Paragraph(vec![test_sentence(
                "It was the best of the times, it was the blurst of times",
            )]),
        ])])),
    )
}

#[test]
pub fn test_owned_block_scope_with_non_block_builder() {
    expect_parse(
        r#"[None]{
It was the best of the times, it was the blurst of times
}
"#,
        Err(TestInterpError::PythonErr {
            pyerr: Regex::new(r"TypeError : Expected object fitting typeclass BlockScopeBuilder, didn't get it. Got None").unwrap(),
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
        r"[TEST_INLINE_BUILDER]{special text}",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::TestOwnedInline(vec![test_text("special text")]),
        ]])])),
    )
}

#[test]
pub fn test_owned_inline_scope_with_non_inline_builder() {
    expect_parse(
        r"[None]{special text}",
        Err(TestInterpError::PythonErr {
            pyerr:
                Regex::new("TypeError : Expected object fitting typeclass InlineScopeBuilder, didn't get it. Got None"
                    ).unwrap(),
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
        r#"[TEST_RAW_INLINE_BUILDER]#{
import os
}#"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::TestOwnedRaw(
                r#"
import os
"#
                .into(),
            ),
        ]])])),
    )
}

#[test]
pub fn test_owned_inline_raw_scope_with_non_raw_builder() {
    expect_parse(
        r#"[None]#{
import os
}#"#,
        Err(TestInterpError::PythonErr {
            pyerr: Regex::new("TypeError : Expected object fitting typeclass RawScopeBuilder, didn't get it. Got None"
        ).unwrap(),
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
            test_sentence("This has a string of"), // The first hash in the chain starts a comment, and trailing whitespace is ignored
        ])])),
    )
}

#[test]
pub fn test_comments() {
    expect_parse(
        r#"It was the best of times. # but...
It was the blurst of times."#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("It was the best of times."),
            test_sentence("It was the blurst of times."),
        ])])),
    )
}

#[test]
pub fn test_special_with_escaped_backslash() {
    expect_parse(
        r#"About to see a backslash! \\#"#,
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![test_text(
            r#"About to see a backslash! \"#,
        )]])])),
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
            TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![test_sentence(
                "block scope",
            )])]),
            TestBlock::Paragraph(vec![vec![TestInline::InlineScope(vec![test_text(
                "inline scope",
            )])]]),
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

#[test]
pub fn test_emit_block_from_code() {
    expect_parse(
        "[TEST_BLOCK]",
        Ok(test_doc(vec![TestBlock::TestOwnedBlock(vec![])])),
    )
}

#[test]
pub fn test_cant_emit_block_from_code_inside_paragraph() {
    expect_parse(
        "Lorem ipsum!
I'm in a [TEST_BLOCK]",
        Err(TestInterpError::BlockCodeMidPara {
            code_span: TestParserSpan {
                start: (2, 10),
                end: (2, 22),
            },
        }),
    )
}

#[test]
pub fn test_raw_scope_emitting_block_from_block_level() {
    expect_parse(
        "[TEST_RAW_BLOCK_BUILDER]#{some raw stuff that goes in a block!}#",
        Ok(test_doc(vec![TestBlock::TestOwnedBlock(vec![])])),
    )
}

#[test]
pub fn test_raw_scope_emitting_inline_from_block_level() {
    expect_parse(
        "[TEST_RAW_INLINE_BUILDER]#{some raw stuff that goes in a block!}#",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::TestOwnedRaw("some raw stuff that goes in a block!".into()),
        ]])])),
    )
}

#[test]
pub fn test_raw_scope_cant_emit_block_inside_paragraph() {
    expect_parse(
        "Inside a paragraph, you can't [TEST_RAW_BLOCK_BUILDER]#{some raw stuff that goes in a block!}#",
        Err(TestInterpError::BlockCodeFromRawScopeMidPara { code_span: TestParserSpan { start: (1, 31), end: (1, 57) } })
    )
}

#[test]
pub fn test_raw_scope_emitting_inline_inside_paragraph() {
    expect_parse(
        "Inside a paragraph, you can [TEST_RAW_INLINE_BUILDER]#{insert an inline raw!}#",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Inside a paragraph, you can "),
            TestInline::TestOwnedRaw("insert an inline raw!".into()),
        ]])])),
    )
}

#[test]
pub fn test_emitting_none_at_block() {
    expect_parse(
        "
[None]
",
        Ok(test_doc(vec![])),
    )
}

#[test]
pub fn test_emitting_none_inline() {
    expect_parse(
        "Check it out, there's [None]!",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            test_text("Check it out, there's "),
            test_text("!"),
        ]])])),
    )
}

#[test]
pub fn test_assign_and_recall() {
    expect_parse(
        "[x = 5]

[x]",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![test_text(
            "5",
        )]])])),
    )
}

#[test]
pub fn test_emit_none() {
    expect_parse("[None]", Ok(test_doc(vec![])))
}

#[test]
pub fn test_cant_eval_none_for_block_builder() {
    expect_parse(
        "[None]{
    That doesn't make any sense! The owner can't be None
}",
        Err(TestInterpError::PythonErr {
            pyerr: Regex::new("TypeError : Expected object fitting typeclass BlockScopeBuilder, didn't get it. Got None").unwrap(),
            code_span: TestParserSpan {
                start: (1, 1),
                end: (2, 1),
            },
        }),
    )
}

#[test]
pub fn test_cant_assign_for_block_builder() {
    expect_parse(
        "[x = 5]{
    That doesn't make any sense! The owner can't be an abstract concept of x being something
}",
        Err(TestInterpError::PythonErr {
            pyerr: Regex::new("TypeError : Expected object fitting typeclass BlockScopeBuilder, didn't get it. Got None").unwrap(),
            code_span: TestParserSpan {
                start: (1, 1),
                end: (2, 1),
            },
        }),
    )
}

#[test]
pub fn test_cant_assign_for_raw_builder() {
    expect_parse(
        "[x = 5]#{That doesn't make any sense! The owner can't be an abstract concept of x being something}#",
        Err(TestInterpError::PythonErr {
            pyerr: Regex::new("TypeError : Expected object fitting typeclass RawScopeBuilder, didn't get it. Got None").unwrap(),
            code_span: TestParserSpan {
                start: (1, 1),
                end: (1, 10),
            },
        }),
    )
}

#[test]
pub fn test_cant_assign_for_inline_builder() {
    expect_parse(
        "[x = 5]{That doesn't make any sense! The owner can't be an abstract concept of x being something}",
        Err(TestInterpError::PythonErr {
            pyerr: Regex::new("TypeError : Expected object fitting typeclass InlineScopeBuilder, didn't get it. Got None").unwrap(),
            code_span: TestParserSpan {
                start: (1, 1),
                end: (1, 9),
            },
        }),
    )
}

#[test]
pub fn test_syntax_errs_passed_thru() {
    // The assignment support depends on trying to eval() the expression, that failing with a SyntaxError, and then trying to exec() it.
    // Make sure that something invalid as both still returns a SyntaxError
    expect_parse(
        "[1invalid]",
        Err(TestInterpError::PythonErr {
            // TODO need to make the equality check for this one use like a prefix or something - this error is not stable between python versions
            pyerr: Regex::new("^SyntaxError : invalid syntax").unwrap(),
            code_span: TestParserSpan {
                start: (1, 1),
                end: (1, 11),
            },
        }),
    )
}

#[test]
pub fn test_block_scope_builder_return_none() {
    expect_parse(
        "[TEST_BLOCK_SWALLOWER]{
stuff that gets swallowed
}",
        Ok(test_doc(vec![])),
    )
}

#[test]
pub fn test_block_scope_builder_return_none_with_end_inside_para() {
    expect_parse(
        "[TEST_BLOCK_SWALLOWER]{
stuff that gets swallowed}",
        Ok(test_doc(vec![])),
    )
}

#[test]
pub fn test_property_calls_get() {
    expect_parse(
        "[TEST_PROPERTY]",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![test_text(
            "5",
        )]])])),
    )
}
