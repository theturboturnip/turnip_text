use crate::error::{TurnipTextError, TurnipTextResult};
use crate::interpreter::InterpError;
use crate::parser::{ParsingFile, TurnipTextParser};
use regex::Regex;

use crate::interpreter::python::{
    interop::{
        BlockScope, DocSegment, DocSegmentHeader, InlineScope, Paragraph, Raw, Sentence, Text,
    },
    prepare_freethreaded_turniptext_python,
};
use crate::util::ParseSpan;

use pyo3::prelude::*;
use pyo3::types::PyDict;

use std::panic;
// We need to initialize Python the first time we test
use std::sync::Once;
static INIT_PYTHON: Once = Once::new();

/// A type mimicking [ParserSpan] for test purposes
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestParserSpan(&'static str);
impl TestParserSpan {
    fn same_text(&self, other: &ParseSpan, data: &Vec<ParsingFile>) -> bool {
        let other_str = unsafe {
            data[other.file_idx()]
                .contents()
                .get_unchecked(other.byte_range())
        };
        dbg!(self.0) == dbg!(other_str)
    }
}

/// A type mimicking [TurnipTextError] for test purposes
#[derive(Debug, Clone)]
pub enum TestTurnipError {
    Lex(char),
    Interp(TestInterpError),
    Internal(Regex),
    InternalPython(Regex),
}
impl From<TestInterpError> for TestTurnipError {
    fn from(value: TestInterpError) -> Self {
        Self::Interp(value)
    }
}
impl PartialEq<TurnipTextError> for TestTurnipError {
    fn eq(&self, other: &TurnipTextError) -> bool {
        match (self, other) {
            (Self::Lex(l_ch), TurnipTextError::Lex(_, _, r_err)) => *l_ch == r_err.ch,
            (Self::Interp(l_interp), TurnipTextError::Interp(sources, r_interp)) => {
                l_interp.effectively_eq(r_interp, sources)
            }
            (Self::InternalPython(l_pyerr), TurnipTextError::InternalPython(r_pyerr)) => {
                l_pyerr.is_match(&r_pyerr)
            }
            (Self::Internal(l_pyerr), TurnipTextError::Internal(r_pyerr)) => {
                l_pyerr.is_match(&r_pyerr)
            }
            _ => false,
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

    DocSegmentHeaderMidPara {
        code_span: TestParserSpan,
    },

    DocSegmentHeaderMidScope {
        code_span: TestParserSpan,
        block_close_span: Option<TestParserSpan>,
        enclosing_scope_start: TestParserSpan,
    },
}
impl TestInterpError {
    fn effectively_eq(&self, other: &InterpError, data: &Vec<ParsingFile>) -> bool {
        match (self, other) {
            (Self::CodeCloseOutsideCode(l0), InterpError::CodeCloseOutsideCode(r0)) => {
                l0.same_text(r0, data)
            }
            (Self::ScopeCloseOutsideScope(l0), InterpError::ScopeCloseOutsideScope(r0)) => {
                l0.same_text(r0, data)
            }
            (
                Self::RawScopeCloseOutsideRawScope(l0),
                InterpError::RawScopeCloseOutsideRawScope(r0),
            ) => l0.same_text(r0, data),
            (
                Self::EndedInsideCode {
                    code_start: l_code_start,
                },
                InterpError::EndedInsideCode {
                    code_start: r_code_start,
                },
            ) => l_code_start.same_text(r_code_start, data),
            (
                Self::EndedInsideRawScope {
                    raw_scope_start: l_raw_scope_start,
                },
                InterpError::EndedInsideRawScope {
                    raw_scope_start: r_raw_scope_start,
                },
            ) => l_raw_scope_start.same_text(r_raw_scope_start, data),
            (
                Self::EndedInsideScope {
                    scope_start: l_scope_start,
                },
                InterpError::EndedInsideScope {
                    scope_start: r_scope_start,
                },
            ) => l_scope_start.same_text(r_scope_start, data),
            (
                Self::BlockScopeOpenedMidPara {
                    scope_start: l_scope_start,
                },
                InterpError::BlockScopeOpenedMidPara {
                    scope_start: r_scope_start,
                },
            ) => l_scope_start.same_text(r_scope_start, data),
            (
                Self::BlockOwnerCodeMidPara {
                    code_span: l_code_span,
                },
                InterpError::BlockOwnerCodeMidPara {
                    code_span: r_code_span,
                },
            ) => l_code_span.same_text(r_code_span, data),
            (
                Self::BlockCodeMidPara {
                    code_span: l_code_span,
                },
                InterpError::BlockCodeMidPara {
                    code_span: r_code_span,
                },
            ) => l_code_span.same_text(r_code_span, data),
            (
                Self::BlockCodeFromRawScopeMidPara {
                    code_span: l_code_span,
                },
                InterpError::BlockCodeFromRawScopeMidPara {
                    code_span: r_code_span,
                },
            ) => l_code_span.same_text(r_code_span, data),
            (
                Self::SentenceBreakInInlineScope {
                    scope_start: l_scope_start,
                },
                InterpError::SentenceBreakInInlineScope {
                    scope_start: r_scope_start,
                },
            ) => l_scope_start.same_text(r_scope_start, data),
            (
                Self::ParaBreakInInlineScope {
                    scope_start: l_scope_start,
                },
                InterpError::ParaBreakInInlineScope {
                    scope_start: r_scope_start,
                    ..
                },
            ) => l_scope_start.same_text(r_scope_start, data),
            (
                Self::BlockOwnerCodeHasNoScope {
                    code_span: l_code_span,
                },
                InterpError::BlockOwnerCodeHasNoScope {
                    code_span: r_code_span,
                },
            ) => l_code_span.same_text(r_code_span, data),
            (
                Self::InlineOwnerCodeHasNoScope {
                    code_span: l_code_span,
                },
                InterpError::InlineOwnerCodeHasNoScope {
                    code_span: r_code_span,
                },
            ) => l_code_span.same_text(r_code_span, data),

            (
                Self::PythonErr {
                    pyerr: l_pyerr,
                    code_span: l_code_span,
                },
                InterpError::PythonErr {
                    ctx: _, // TODO this is user-facing - do we need to test it?
                    pyerr: r_pyerr,
                    code_span: r_code_span,
                },
            ) => dbg!(l_pyerr).is_match(&dbg!(r_pyerr)) && l_code_span.same_text(r_code_span, data),

            (
                Self::EscapedNewlineOutsideParagraph { newline: l_newline },
                InterpError::EscapedNewlineOutsideParagraph { newline: r_newline },
            ) => l_newline.same_text(r_newline, data),

            (
                Self::DocSegmentHeaderMidPara {
                    code_span: l_code_span,
                },
                InterpError::DocSegmentHeaderMidPara {
                    code_span: r_code_span,
                },
            ) => l_code_span.same_text(r_code_span, data),

            (
                Self::DocSegmentHeaderMidScope {
                    code_span: l_code_span,
                    block_close_span: l_block_close_span,
                    enclosing_scope_start: l_enclosing_scope_start,
                },
                InterpError::DocSegmentHeaderMidScope {
                    code_span: r_code_span,
                    block_close_span: r_block_close_span,
                    enclosing_scope_start: r_enclosing_scope_start,
                },
            ) => {
                (l_code_span.same_text(r_code_span, data))
                    && (match (l_block_close_span, r_block_close_span) {
                        (Some(l), Some(r)) => l.same_text(r, data),
                        (None, None) => true,
                        _ => false,
                    })
                    && (l_enclosing_scope_start.same_text(r_enclosing_scope_start, data))
            }
            _ => false,
        }
    }
}

const GLOBALS_CODE: &'static str = r#"
# The Rust module name is _native, which is included under turnip_text, so Python IDEs don't try to import directly from it.
# This means we use _native instead of turnip_text as the module name here.
from _native import InlineScope, Text, BlockScope

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

class TestDocSegmentHeader:
    is_segment_header = True
    weight = 0
    def __init__(self, test_block=None):
        self.test_block = test_block

class TestDocSegmentBuilder:
    def build_from_blocks(self, contents):
        return TestDocSegmentHeader(contents)
"#;

#[derive(Debug, PartialEq, Eq)]
pub struct TestDocSegment {
    header: Option<(i64, Option<TestBlock>)>,
    contents: TestBlock,
    subsegments: Vec<TestDocSegment>,
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
    Text(String),
    Raw(String),

    /// Test-only - a Python object built from an inline scope with test_inline: List[Inline] = the contents of that scope
    TestOwnedInline(Vec<TestInline>),
    /// Test-only - a Python object built from raw text with test_raw_str: str = the raw text
    TestOwnedRaw(String),
}
pub fn test_doc(contents: Vec<TestBlock>) -> TestDocSegment {
    TestDocSegment {
        header: None,
        contents: TestBlock::BlockScope(contents),
        subsegments: vec![],
    }
}
pub fn test_sentence(s: impl Into<String>) -> Vec<TestInline> {
    vec![TestInline::Text(s.into())]
}
pub fn test_text(s: impl Into<String>) -> TestInline {
    TestInline::Text(s.into())
}
pub fn test_raw_text(s: impl Into<String>) -> TestInline {
    TestInline::Raw(s.into())
}

pub trait PyToTest<T> {
    fn as_test(&self, py: Python) -> T;
}
impl PyToTest<TestDocSegment> for PyAny {
    fn as_test(&self, py: Python) -> TestDocSegment {
        if let Ok(doc_segment) = self.extract::<DocSegment>() {
            TestDocSegment {
                header: doc_segment.header.map(|header| {
                    let weight = DocSegmentHeader::get_weight(py, header.as_ref(py))
                        .expect("Couldn't get_weight of header");
                    let contents = match header.as_ref(py).getattr("test_block") {
                        Ok(test_block) => {
                            if test_block.is_none() {
                                None
                            } else {
                                Some(test_block.as_test(py))
                            }
                        }
                        Err(_) => None,
                    };
                    (weight, contents)
                }),
                contents: doc_segment.contents.as_ref(py).as_test(py),
                subsegments: doc_segment
                    .subsegments
                    .list(py)
                    .iter()
                    .map(|subseg| subseg.as_test(py))
                    .collect(),
            }
        } else {
            let repr = match self.repr() {
                Ok(py_str) => py_str.to_string(),
                Err(_) => "<couldn't call __repr__>".to_owned(),
            };
            panic!("Expected DocSegment, got {repr}")
        }
    }
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
            let repr = match self.repr() {
                Ok(py_str) => py_str.to_string(),
                Err(_) => "<couldn't call __repr__>".to_owned(),
            };
            panic!("Expected BlockNode-like, got {repr}")
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
            let repr = match self.repr() {
                Ok(py_str) => py_str.to_string(),
                Err(_) => "<couldn't call __repr__>".to_owned(),
            };
            panic!("Expected Sentence, got {repr}")
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
        } else if let Ok(text) = self.extract::<Text>() {
            TestInline::Text(text.0.as_ref(py).to_string())
        } else if let Ok(text) = self.extract::<Raw>() {
            TestInline::Raw(text.0.as_ref(py).to_string())
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
            let repr = match self.repr() {
                Ok(py_str) => py_str.to_string(),
                Err(_) => "<couldn't call __repr__>".to_owned(),
            };
            panic!("Expected Inline, got {repr}")
        }
    }
}

/// Generate a set of local Python variables used in each test case
///
/// Provides `TEST_BLOCK_BUILDER`, `TEST_INLINE_BUILDER`, `TEST_RAW_BUILDER` objects
/// that can own block, inline, and raw scopes respectively.
pub fn generate_globals<'interp>(py: Python<'interp>) -> Option<&'interp PyDict> {
    let globals = PyDict::new(py);

    let result = py.run(GLOBALS_CODE, Some(globals), Some(globals));

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

fn expect_parse_err<T: Into<TestTurnipError>>(data: &str, expected_err: T) {
    expect_parse(data, Err(expected_err.into()))
}

pub fn expect_parse(data: &str, expected_parse: Result<TestDocSegment, TestTurnipError>) {
    // Make sure Python has been set up
    INIT_PYTHON.call_once(prepare_freethreaded_turniptext_python);

    // Second step: parse
    // Need to do this safely so that we don't panic inside Python::with_gil.
    // I'm not 100% sure but I'm afraid it will poison the GIL and break subsequent tests.
    let root: Result<TurnipTextResult<TestDocSegment>, _> = {
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
    expect_parse_err(
        r#"[None]{
It was the best of the times, it was the blurst of times
}
"#,
        TestInterpError::PythonErr {
            pyerr: Regex::new(r"TypeError : Expected object fitting typeclass BlockScopeBuilder, didn't get it. Got None").unwrap(),
            code_span: TestParserSpan("[None]{\n"),
        },
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
    expect_parse_err(
        r"[None]{special text}",
        TestInterpError::PythonErr {
            pyerr:
                Regex::new("TypeError : Expected object fitting typeclass InlineScopeBuilder, didn't get it. Got None"
                    ).unwrap(),
            code_span: TestParserSpan("[None]{"),
        },
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
    expect_parse_err(
        r#"[None]#{
import os
}#"#,
        TestInterpError::PythonErr {
            pyerr: Regex::new("TypeError : Expected object fitting typeclass RawScopeBuilder, didn't get it. Got None"
        ).unwrap(),
            code_span: TestParserSpan("[None]#{"),
        },
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
    expect_parse_err(
        "not code ] but closed code",
        TestInterpError::CodeCloseOutsideCode(TestParserSpan("]")),
    )
}
#[test]
pub fn test_inline_scope_close_outside_scope() {
    expect_parse_err(
        "not in a scope } but closed scope",
        TestInterpError::ScopeCloseOutsideScope(TestParserSpan("}")),
    )
}
#[test]
pub fn test_block_scope_close_outside_scope() {
    expect_parse_err(
        "} # not in a scope",
        TestInterpError::ScopeCloseOutsideScope(TestParserSpan("}")),
    )
}
#[test]
pub fn test_mismatching_raw_scope_close() {
    expect_parse_err(
        "##{ text in a scope with a }#",
        TestInterpError::EndedInsideRawScope {
            raw_scope_start: TestParserSpan("##{"),
        },
    )
}
#[test]
pub fn test_ended_inside_code() {
    expect_parse_err(
        "text [code",
        TestInterpError::EndedInsideCode {
            code_start: TestParserSpan("["),
        },
    )
}
#[test]
pub fn test_ended_inside_raw_scope() {
    expect_parse_err(
        "text #{raw",
        TestInterpError::EndedInsideRawScope {
            raw_scope_start: TestParserSpan("#{"),
        },
    )
}
#[test]
pub fn test_ended_inside_scope() {
    expect_parse_err(
        "text {scope",
        TestInterpError::EndedInsideScope {
            scope_start: TestParserSpan("{"),
        },
    )
}
#[test]
pub fn test_newline_inside_inline_scope() {
    expect_parse_err(
        "text {scope\n",
        TestInterpError::SentenceBreakInInlineScope {
            scope_start: TestParserSpan("{"),
        },
    )
}
#[test]
pub fn test_block_scope_open_inline() {
    expect_parse_err(
        "text {\n",
        TestInterpError::SentenceBreakInInlineScope {
            scope_start: TestParserSpan("{\n"),
        },
    )
}
#[test]
pub fn test_eof_inside_para_inside_block_scope() {
    // Under some broken parsers the EOF would implicitly end the paragraph but would be stopped there - it wouldn't be picked up by the block scope.
    expect_parse_err(
        "{\n paragraph paragraph paragraph EOF",
        TestInterpError::EndedInsideScope {
            scope_start: TestParserSpan("{\n"),
        },
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
            "No whitespace allowed after this! \n",
            "I mean it!                        "
        ),
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("No whitespace allowed after this!"),
            test_sentence("I mean it!"),
        ])])),
    )
}

#[test]
pub fn test_strip_leading_scope_whitespace() {
    expect_parse(
        "{ no leading whitespace}",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::InlineScope(vec![test_text("no leading whitespace")]),
        ]])])),
    )
}

#[test]
pub fn test_strip_trailing_scope_whitespace() {
    expect_parse(
        "{no trailing whitespace }",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::InlineScope(vec![test_text("no trailing whitespace")]),
        ]])])),
    )
}

#[test]
pub fn test_dont_strip_whitespace_between_scopes() {
    expect_parse(
        "{ stuff }     { other stuff }",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::InlineScope(vec![test_text("stuff")]),
            test_text("     "),
            TestInline::InlineScope(vec![test_text("other stuff")]),
        ]])])),
    )
}

#[test]
pub fn test_strip_whitespace_after_scope() {
    expect_parse(
        "{ stuff }     \n",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::InlineScope(vec![test_text("stuff")]),
        ]])])),
    )
}

#[test]
pub fn test_strip_whitespace_between_scope_end_and_comment() {
    expect_parse(
        "{ stuff }     # stuff in a comment!\n",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![vec![
            TestInline::InlineScope(vec![test_text("stuff")]),
        ]])])),
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
    expect_parse_err(
        "Lorem ipsum!
I'm in a [TEST_BLOCK]",
        TestInterpError::BlockCodeMidPara {
            code_span: TestParserSpan("[TEST_BLOCK]"),
        },
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
    expect_parse_err(
        "Inside a paragraph, you can't [TEST_RAW_BLOCK_BUILDER]#{some raw stuff that goes in a block!}#",
        TestInterpError::BlockCodeFromRawScopeMidPara { code_span: TestParserSpan("[TEST_RAW_BLOCK_BUILDER]#{") }
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
    expect_parse_err(
        "[None]{
    That doesn't make any sense! The owner can't be None
}",
        TestInterpError::PythonErr {
            pyerr: Regex::new("TypeError : Expected object fitting typeclass BlockScopeBuilder, didn't get it. Got None").unwrap(),
            code_span: TestParserSpan("[None]{\n"),
        },
    )
}

#[test]
pub fn test_cant_assign_for_block_builder() {
    expect_parse_err(
        "[x = 5]{
    That doesn't make any sense! The owner can't be an abstract concept of x being something
}",
        TestInterpError::PythonErr {
            pyerr: Regex::new("TypeError : Expected object fitting typeclass BlockScopeBuilder, didn't get it. Got None").unwrap(),
            code_span: TestParserSpan("[x = 5]{\n"),
        },
    )
}

#[test]
pub fn test_cant_assign_for_raw_builder() {
    expect_parse_err(
        "[x = 5]#{That doesn't make any sense! The owner can't be an abstract concept of x being something}#",
        TestInterpError::PythonErr {
            pyerr: Regex::new("TypeError : Expected object fitting typeclass RawScopeBuilder, didn't get it. Got None").unwrap(),
            code_span: TestParserSpan("[x = 5]#{"),
        },
    )
}

#[test]
pub fn test_cant_assign_for_inline_builder() {
    expect_parse_err(
        "[x = 5]{That doesn't make any sense! The owner can't be an abstract concept of x being something}",
        TestInterpError::PythonErr {
            pyerr: Regex::new("TypeError : Expected object fitting typeclass InlineScopeBuilder, didn't get it. Got None").unwrap(),
            code_span: TestParserSpan ("[x = 5]{"),
        },
    )
}

#[test]
pub fn test_syntax_errs_passed_thru() {
    // The assignment support depends on trying to eval() the expression, that failing with a SyntaxError, and then trying to exec() it.
    // Make sure that something invalid as both still returns a SyntaxError
    expect_parse_err(
        "[1invalid]",
        TestInterpError::PythonErr {
            pyerr: Regex::new("^SyntaxError : invalid syntax").unwrap(),
            code_span: TestParserSpan("[1invalid]"),
        },
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
stuff that gets swallowed
}",
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

#[test]
pub fn test_no_emit_doc_segment_header_in_block_scope() {
    expect_parse_err(
        "{
[TestDocSegmentHeader()]
}",
        TestInterpError::DocSegmentHeaderMidScope {
            code_span: TestParserSpan("[TestDocSegmentHeader()]"),
            block_close_span: None,
            enclosing_scope_start: TestParserSpan("{\n"),
        },
    )
}

#[test]
pub fn test_no_build_doc_segment_header_in_block_scope() {
    expect_parse_err(
        "{
[TestDocSegmentBuilder()]{
    Sometimes docsegmentheaders can be built, too!
    But if they're in a block scope it shouldn't be allowed :(
}
}",
        TestInterpError::DocSegmentHeaderMidScope {
            code_span: TestParserSpan("[TestDocSegmentBuilder()]{\n"),
            block_close_span: Some(TestParserSpan("}")),
            enclosing_scope_start: TestParserSpan("{\n"),
        },
    )
}

#[test]
pub fn test_no_emit_doc_segment_header_in_para() {
    expect_parse_err(
        "And as I was saying [TestDocSegmentHeader()]",
        TestInterpError::DocSegmentHeaderMidPara {
            code_span: TestParserSpan("[TestDocSegmentHeader()]"),
        },
    )
}

/*
// These are tests for strict blank-line syntax checking - where the parser ensures that there is always a blank line between two blocks.
// With the way the parser is currently structured, it's impossible to check this inside subfiles without having the newlines inside subfiles impact the correctness of the surrounding file.
// Thus these tests are disabled and instead we allow some funky unintuitive syntax.

// There should always be a blank line between a paragraph ending and a paragraph starting
// (otherwise they'd be the same paragraph)
#[test]
pub fn test_block_sep_para_para() {
    expect_parse(
        "Paragraph one\n has some content\n\nThis is paragraph two",
        Ok(test_doc(vec![
            TestBlock::Paragraph(vec![
                test_sentence("Paragraph one"),
                test_sentence("has some content"),
            ]),
            TestBlock::Paragraph(vec![test_sentence("This is paragraph two")]),
        ])),
    );
    expect_parse(
        "Paragraph one\nhas some content\nThis isn't paragraph two!",
        Ok(test_doc(vec![TestBlock::Paragraph(vec![
            test_sentence("Paragraph one"),
            test_sentence("has some content"),
            test_sentence("This isn't paragraph two!"),
        ])])),
    )
}

// There should always be a blank line between a paragraph ending and a block scope starting
#[test]
pub fn test_block_sep_para_scope_open() {
    expect_parse(
        r#"Paragraph one

        {
            New Block
        }"#,
        Ok(test_doc(vec![
            TestBlock::Paragraph(vec![test_sentence("Paragraph one")]),
            TestBlock::BlockScope(vec![TestBlock::Paragraph(vec![test_sentence("New Block")])]),
        ])),
    );
    expect_parse_err(
        r#"Paragraph one
        {
            New Block
        }"#,
        TestInterpError::InsufficientBlockSeparation {
            block_start: TestParserSpan("{\n"),
        },
    )
}

// There should always be a blank line between a paragraph ending and code-emitting-block
// - this is picked up as trying to emit a block inside a paragraph
#[test]
pub fn test_block_sep_para_code() {
    expect_parse(
        r#"Paragraph one

        [TEST_BLOCK]"#,
        Ok(test_doc(vec![
            TestBlock::Paragraph(vec![test_sentence("Paragraph one")]),
            TestBlock::TestOwnedBlock(vec![]),
        ])),
    );
    expect_parse_err(
        r#"Paragraph one
        [TEST_BLOCK]"#,
        TestInterpError::BlockCodeMidPara {
            code_span: TestParserSpan("[TEST_BLOCK]"),
        },
    )
}

// There should always be a blank line between a code-emitting-block and a paragraph starting
#[test]
pub fn test_block_sep_code_para() {
    expect_parse(
        r#"[TEST_BLOCK]

        Paragraph one"#,
        Ok(test_doc(vec![
            TestBlock::TestOwnedBlock(vec![]),
            TestBlock::Paragraph(vec![test_sentence("Paragraph one")]),
        ])),
    );
    expect_parse_err(
        r#"[TEST_BLOCK]
        Paragraph one"#,
        TestInterpError::InsufficientBlockSeparation {
            block_start: TestParserSpan("P"),
        },
    )
}

// This should *not* trigger insufficient space - it's fine to close a block scope directly after a paragraph
#[test]
pub fn test_block_sep_para_scope_close() {
    expect_parse(
        r#"{
            Paragraph one
        }"#,
        Ok(test_doc(vec![TestBlock::BlockScope(vec![
            TestBlock::Paragraph(vec![test_sentence("Paragraph one")]),
        ])])),
    );
}

// There should always be a blank line between a scope closing and another scope starting
#[test]
pub fn test_block_sep_scope_scope() {
    expect_parse(
        r#"{
        }

        {
        }"#,
        Ok(test_doc(vec![
            TestBlock::BlockScope(vec![]),
            TestBlock::BlockScope(vec![]),
        ])),
    );
    expect_parse_err(
        r#"{
        }
        {
        }"#,
        TestInterpError::InsufficientBlockSeparation {
            block_start: TestParserSpan("{\n"),
        },
    )
}

// There should always be a blank line between a scope closing and code-emitting-block
#[test]
pub fn test_block_sep_scope_code() {
    expect_parse(
        r#"{
        }

        [TEST_BLOCK]"#,
        Ok(test_doc(vec![
            TestBlock::BlockScope(vec![]),
            TestBlock::TestOwnedBlock(vec![]),
        ])),
    );
    expect_parse_err(
        r#"{
        }
        [TEST_BLOCK]"#,
        TestInterpError::InsufficientBlockSeparation {
            block_start: TestParserSpan("["),
        },
    )
}

// There should always be a blank line between a code-emitting-block and a scope opening
#[test]
pub fn test_block_sep_code_scope() {
    expect_parse(
        r#"
        [TEST_BLOCK]

        {
        }"#,
        Ok(test_doc(vec![
            TestBlock::TestOwnedBlock(vec![]),
            TestBlock::BlockScope(vec![]),
        ])),
    );
    expect_parse_err(
        r#"
        [TEST_BLOCK]
        {
        }"#,
        TestInterpError::InsufficientBlockSeparation {
            block_start: TestParserSpan("{\n"),
        },
    )
}

// There should always be a blank line between two code-emitting-blocks
#[test]
pub fn test_block_sep_code_code() {
    expect_parse(
        r#"
        [TEST_BLOCK]

        [TEST_BLOCK]"#,
        Ok(test_doc(vec![
            TestBlock::TestOwnedBlock(vec![]),
            TestBlock::TestOwnedBlock(vec![]),
        ])),
    );
    expect_parse_err(
        r#"
        [TEST_BLOCK]
        [TEST_BLOCK]"#,
        TestInterpError::InsufficientBlockSeparation {
            block_start: TestParserSpan("["),
        },
    )
}
*/

// TODO MORE TESTS FOR DOC STURCURE. OH FUCK I NEED TO CHANGE THE TEST HARNESS
// TODO tests for inserted files. Are they exclusively on block boundaries?
