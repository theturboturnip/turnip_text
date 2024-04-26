//! This module provides helper functions and types for mimicking "real" turnip-text data structures (especially those created in Python) in Rust.
//! The general usage pattern is to define the expected result of your test with these types, then for harness code to execute the necessary Rust+Python and to then convert those results to these types before comparing.

use crate::error::interp::{BlockModeElem, InlineModeContext};
use crate::error::lexer::LexError;
use crate::error::{interp::InterpError, TurnipTextError};
use crate::error::{stringify_pyerr, UserPythonExecError};
use crate::interpreter::ParsingFile;
use regex::Regex;

use crate::python::interop::{
    BlockScope, DocSegment, DocSegmentHeader, InlineScope, Paragraph, Raw, Sentence, Text,
};
use crate::util::{ParseContext, ParseSpan};

use pyo3::prelude::*;
use pyo3::types::PyDict;

/// A type mimicking [ParserSpan] for test purposes
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestParseSpan<'a>(pub &'a str);
impl<'a> From<(&ParseSpan, &'a Vec<ParsingFile>)> for TestParseSpan<'a> {
    fn from(value: (&ParseSpan, &'a Vec<ParsingFile>)) -> Self {
        Self(unsafe {
            value.1[value.0.file_idx()]
                .contents()
                .get_unchecked(value.0.byte_range())
        })
    }
}

/// A type mimicking [ParserContext] for test purposes
///
/// .0 = first token
/// .1 = intervening tokens
/// .2 = last token
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestParseContext<'a>(pub &'a str, pub &'a str, pub &'a str);
impl<'a> From<(&ParseContext, &'a Vec<ParsingFile>)> for TestParseContext<'a> {
    fn from(value: (&ParseContext, &'a Vec<ParsingFile>)) -> Self {
        let start: TestParseSpan = (&value.0.first_tok(), value.1).into();

        let middle: TestParseSpan =
            if value.0.first_tok().end().byte_ofs <= value.0.last_tok().start().byte_ofs {
                let middle_span = ParseSpan::new(
                    value.0.first_tok().file_idx(),
                    value.0.first_tok().end(),
                    value.0.last_tok().start(),
                );
                (&middle_span, value.1).into()
            } else {
                TestParseSpan("")
            };

        let end: TestParseSpan = (&value.0.last_tok(), value.1).into();
        Self(start.0, middle.0, end.0)
    }
}

/// A type mimicking [TurnipTextError] for test purposes
///
/// Does not derive
#[derive(Debug, Clone)]
pub enum TestTurnipError<'a> {
    Lex(TestLexError<'a>),
    Interp(TestInterpError<'a>),
    UserPython(TestUserPythonExecError<'a>),
    InternalPython(Regex),
}
impl<'a> From<TestLexError<'a>> for TestTurnipError<'a> {
    fn from(value: TestLexError<'a>) -> Self {
        Self::Lex(value)
    }
}
impl<'a> From<TestInterpError<'a>> for TestTurnipError<'a> {
    fn from(value: TestInterpError<'a>) -> Self {
        Self::Interp(value)
    }
}
impl<'a> From<TestUserPythonExecError<'a>> for TestTurnipError<'a> {
    fn from(value: TestUserPythonExecError<'a>) -> Self {
        Self::UserPython(value)
    }
}
impl<'a> TestTurnipError<'a> {
    pub fn matches(&self, py: Python, other: &TurnipTextError) -> bool {
        match (self, other) {
            (Self::Lex(expected), TurnipTextError::Lex(sources, actual)) => {
                let actual_as_test: TestLexError<'_> = (actual, sources).into();
                *dbg!(expected) == dbg!(actual_as_test)
            }
            (Self::Interp(expected), TurnipTextError::Interp(sources, actual)) => {
                let actual_as_test: TestInterpError<'_> = (actual, sources).into();
                *dbg!(expected) == dbg!(actual_as_test)
            }
            (Self::UserPython(l_err), TurnipTextError::UserPython(sources, r_err)) => {
                l_err.matches(py, r_err, sources)
            }
            (Self::InternalPython(l_pyerr), TurnipTextError::InternalPython(r_pyerr)) => {
                l_pyerr.is_match(&r_pyerr)
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TestLexError<'a> {
    TooLongStringOfHyphenMinus(TestParseSpan<'a>, usize),
}
impl<'a> From<(&'a LexError, &'a Vec<ParsingFile>)> for TestLexError<'a> {
    fn from(value: (&'a LexError, &'a Vec<ParsingFile>)) -> Self {
        match value.0 {
            LexError::TooLongStringOfHyphenMinus(span, n) => {
                TestLexError::TooLongStringOfHyphenMinus((span, value.1).into(), *n)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TestInlineModeContext<'a> {
    Paragraph(TestParseContext<'a>),
    InlineScope { scope_start: TestParseSpan<'a> },
}
impl<'a> From<(&'a InlineModeContext, &'a Vec<ParsingFile>)> for TestInlineModeContext<'a> {
    fn from(value: (&'a InlineModeContext, &'a Vec<ParsingFile>)) -> Self {
        match value.0 {
            InlineModeContext::Paragraph(c) => Self::Paragraph((c, value.1).into()),
            InlineModeContext::InlineScope { scope_start } => Self::InlineScope {
                scope_start: (scope_start, value.1).into(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TestBlockModeElem<'a> {
    HeaderFromCode(TestParseSpan<'a>),
    Para(TestParseContext<'a>),
    BlockScope(TestParseContext<'a>),
    BlockFromCode(TestParseSpan<'a>),
    SourceFromCode(TestParseSpan<'a>),
    AnyToken(TestParseSpan<'a>),
}
impl<'a> From<(&'a BlockModeElem, &'a Vec<ParsingFile>)> for TestBlockModeElem<'a> {
    fn from(value: (&'a BlockModeElem, &'a Vec<ParsingFile>)) -> Self {
        match value.0 {
            BlockModeElem::HeaderFromCode(s) => Self::HeaderFromCode((s, value.1).into()),
            BlockModeElem::Para(c) => Self::Para((c, value.1).into()),
            BlockModeElem::BlockScope(c) => Self::BlockScope((c, value.1).into()),
            BlockModeElem::BlockFromCode(s) => Self::BlockFromCode((s, value.1).into()),
            BlockModeElem::SourceFromCode(s) => Self::SourceFromCode((s, value.1).into()),
            BlockModeElem::AnyToken(s) => Self::AnyToken((s, value.1).into()),
        }
    }
}

/// A type mimicking [InterpError] for test purposes
#[derive(Debug, Clone, PartialEq)]
pub enum TestInterpError<'a> {
    CodeCloseOutsideCode(TestParseSpan<'a>),
    BlockScopeCloseOutsideScope(TestParseSpan<'a>),
    InlineScopeCloseOutsideScope(TestParseSpan<'a>),
    RawScopeCloseOutsideRawScope(TestParseSpan<'a>),
    EndedInsideCode {
        code_start: TestParseSpan<'a>,
        eof_span: TestParseSpan<'a>,
    },
    EndedInsideRawScope {
        raw_scope_start: TestParseSpan<'a>,
        eof_span: TestParseSpan<'a>,
    },
    EndedInsideScope {
        scope_start: TestParseSpan<'a>,
        eof_span: TestParseSpan<'a>,
    },
    BlockScopeOpenedInInlineMode {
        inl_mode: TestInlineModeContext<'a>,
        block_scope_open: TestParseSpan<'a>,
    },
    CodeEmittedBlockInInlineMode {
        inl_mode: TestInlineModeContext<'a>,
        code_span: TestParseSpan<'a>,
    },
    CodeEmittedHeaderInInlineMode {
        inl_mode: TestInlineModeContext<'a>,
        code_span: TestParseSpan<'a>,
    },
    CodeEmittedHeaderInBlockScope {
        block_scope_start: TestParseSpan<'a>,
        code_span: TestParseSpan<'a>, // TODO should include argument to code_span separately
    },
    CodeEmittedSourceInInlineMode {
        inl_mode: TestInlineModeContext<'a>,
        code_span: TestParseSpan<'a>,
    },
    SentenceBreakInInlineScope {
        scope_start: TestParseSpan<'a>,
        sentence_break: TestParseSpan<'a>,
    },
    EscapedNewlineOutsideParagraph {
        newline: TestParseSpan<'a>,
    },
    InsufficientBlockSeparation {
        last_block: TestBlockModeElem<'a>,
        next_block_start: TestBlockModeElem<'a>,
    },
}
impl<'a> From<(&'a Box<InterpError>, &'a Vec<ParsingFile>)> for TestInterpError<'a> {
    fn from(value: (&'a Box<InterpError>, &'a Vec<ParsingFile>)) -> Self {
        match value.0.as_ref() {
            InterpError::CodeCloseOutsideCode(s) => Self::CodeCloseOutsideCode((s, value.1).into()),
            InterpError::BlockScopeCloseOutsideScope(s) => {
                Self::BlockScopeCloseOutsideScope((s, value.1).into())
            }
            InterpError::InlineScopeCloseOutsideScope(s) => {
                Self::InlineScopeCloseOutsideScope((s, value.1).into())
            }
            InterpError::RawScopeCloseOutsideRawScope(s) => {
                Self::RawScopeCloseOutsideRawScope((s, value.1).into())
            }
            InterpError::EndedInsideCode {
                code_start,
                eof_span,
            } => Self::EndedInsideCode {
                code_start: (code_start, value.1).into(),
                eof_span: (eof_span, value.1).into(),
            },
            InterpError::EndedInsideRawScope {
                raw_scope_start,
                eof_span,
            } => Self::EndedInsideRawScope {
                raw_scope_start: (raw_scope_start, value.1).into(),
                eof_span: (eof_span, value.1).into(),
            },
            InterpError::EndedInsideScope {
                scope_start,
                eof_span,
            } => Self::EndedInsideScope {
                scope_start: (scope_start, value.1).into(),
                eof_span: (eof_span, value.1).into(),
            },
            InterpError::BlockScopeOpenedInInlineMode {
                inl_mode,
                block_scope_open,
            } => Self::BlockScopeOpenedInInlineMode {
                inl_mode: (inl_mode, value.1).into(),
                block_scope_open: (block_scope_open, value.1).into(),
            },
            InterpError::CodeEmittedBlockInInlineMode {
                inl_mode,
                block: _,
                code_span,
            } => Self::CodeEmittedBlockInInlineMode {
                inl_mode: (inl_mode, value.1).into(),
                code_span: (code_span, value.1).into(),
            },
            InterpError::CodeEmittedHeaderInInlineMode {
                inl_mode,
                header: _,
                code_span,
            } => Self::CodeEmittedHeaderInInlineMode {
                inl_mode: (inl_mode, value.1).into(),
                code_span: (code_span, value.1).into(),
            },
            InterpError::CodeEmittedHeaderInBlockScope {
                block_scope_start,
                header: _,
                code_span,
            } => Self::CodeEmittedHeaderInBlockScope {
                block_scope_start: (block_scope_start, value.1).into(),
                code_span: (code_span, value.1).into(),
            },
            InterpError::CodeEmittedSourceInInlineMode {
                inl_mode,
                code_span,
            } => Self::CodeEmittedSourceInInlineMode {
                inl_mode: (inl_mode, value.1).into(),
                code_span: (code_span, value.1).into(),
            },
            InterpError::SentenceBreakInInlineScope {
                scope_start,
                sentence_break,
            } => Self::SentenceBreakInInlineScope {
                scope_start: (scope_start, value.1).into(),
                sentence_break: (sentence_break, value.1).into(),
            },

            InterpError::EscapedNewlineOutsideParagraph { newline } => {
                Self::EscapedNewlineOutsideParagraph {
                    newline: (newline, value.1).into(),
                }
            }
            InterpError::InsufficientBlockSeparation {
                last_block,
                next_block_start,
            } => Self::InsufficientBlockSeparation {
                last_block: (last_block, value.1).into(),
                next_block_start: (next_block_start, value.1).into(),
            },
        }
    }
}

/// The contexts in which you might execute Python on user-generated code or objects
#[derive(Debug, Clone)]
pub enum TestUserPythonExecError<'a> {
    RunningEvalBrackets {
        code: TestParseContext<'a>,
        err: Regex,
    },
    CoercingNonBuilderEvalBracket {
        code: TestParseContext<'a>,
    },
    CoercingBlockScopeBuilder {
        code: TestParseContext<'a>,
        err: Regex,
    },
    CoercingInlineScopeBuilder {
        code: TestParseContext<'a>,
        err: Regex,
    },
    CoercingRawScopeBuilder {
        code: TestParseContext<'a>,
        err: Regex,
    },
}
impl<'a> TestUserPythonExecError<'a> {
    pub fn matches(
        &self,
        py: Python,
        other: &'a UserPythonExecError,
        data: &'a Vec<ParsingFile>,
    ) -> bool {
        match (self, other) {
            (
                TestUserPythonExecError::RunningEvalBrackets {
                    code: l_code,
                    err: l_err,
                },
                UserPythonExecError::RunningEvalBrackets {
                    code: r_code,
                    err: r_err,
                },
            )
            | (
                TestUserPythonExecError::CoercingBlockScopeBuilder {
                    code: l_code,
                    err: l_err,
                },
                UserPythonExecError::CoercingBlockScopeBuilder {
                    code: r_code,
                    err: r_err,
                    obj: _,
                },
            )
            | (
                TestUserPythonExecError::CoercingInlineScopeBuilder {
                    code: l_code,
                    err: l_err,
                },
                UserPythonExecError::CoercingInlineScopeBuilder {
                    code: r_code,
                    err: r_err,
                    obj: _,
                },
            )
            | (
                TestUserPythonExecError::CoercingRawScopeBuilder {
                    code: l_code,
                    err: l_err,
                },
                UserPythonExecError::CoercingRawScopeBuilder {
                    code: r_code,
                    err: r_err,
                    obj: _,
                },
            ) => {
                (*dbg!(l_code) == dbg!((r_code, data).into()))
                    && dbg!(l_err).is_match(&dbg!(stringify_pyerr(py, r_err)))
            }

            (
                TestUserPythonExecError::CoercingNonBuilderEvalBracket { code: l_code },
                UserPythonExecError::CoercingNonBuilderEvalBracket {
                    code: r_code,
                    obj: _,
                },
            ) => *dbg!(l_code) == dbg!((r_code, data).into()),
            _ => false,
        }
    }
}

pub const GLOBALS_CODE: &'static str = r#"
# The Rust module name is _native, which is included under turnip_text, so Python IDEs don't try to import directly from it.
# This means we use _native instead of turnip_text as the module name here.
from _native import InlineScope, Text, BlockScope, TurnipTextSource, Paragraph, Sentence, Raw

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

class TestBlockBuilder:
    def build_from_blocks(self, contents):
        return FauxBlock(contents)
    
class TestInlineBuilder:
    def build_from_inlines(self, contents):
        return FauxInline(contents)

class TestRawInlineBuilder:
    def build_from_raw(self, raw_str):
        return FauxInlineRaw(raw_str)

TEST_BLOCK = FauxBlock(BlockScope([]))
TEST_INLINE = FauxInline(InlineScope([]))
TEST_INLINE_RAW = FauxInlineRaw("")

class TestRawBlockBuilder:
    def build_from_raw(self, raw_str):
        return TEST_BLOCK

class TestBlockSwallower():
    def build_from_blocks(self, contents):
        return None
class TestInlineSwallower():
    def build_from_inlines(self, contents):
        return None
class TestRawSwallower():
    def build_from_raw(self, raw):
        return None

TEST_BLOCK_BUILDER = TestBlockBuilder()
TEST_INLINE_BUILDER = TestInlineBuilder()
TEST_RAW_INLINE_BUILDER = TestRawInlineBuilder()
TEST_RAW_BLOCK_BUILDER = TestRawBlockBuilder()

TEST_BLOCK_SWALLOWER = TestBlockSwallower()
TEST_INLINE_SWALLOWER = TestInlineSwallower()
TEST_RAW_SWALLOWER = TestRawSwallower()

TEST_PROPERTY = property(lambda x: 5)

class TestDocSegmentHeader:
    is_segment_header = True
    weight = 0
    def __init__(self, weight=0, test_block=None, test_inline=None):
        self.weight = weight
        self.test_block = test_block
        self.test_inline = test_inline

class TestDocSegmentBuilder:
    def __init__(self, weight=0):
        self.weight=weight
    def build_from_blocks(self, contents):
        return TestDocSegmentHeader(weight=self.weight, test_block=contents)
    def build_from_inlines(self, contents):
        return TestDocSegmentHeader(weight=self.weight, test_inline=contents)
    def build_from_raw(self, raw):
        return TestDocSegmentHeader(weight=self.weight, test_inline=InlineScope([Raw(raw)]))

def test_src(contents):
    return TurnipTextSource.from_string(contents)

class TestBlockBuilderFromInline:
    def build_from_inlines(self, contents: InlineScope):
        return FauxBlock(BlockScope([Paragraph([Sentence([contents])])]))

TEST_BLOCK_BUILDER_FROM_INLINE = TestBlockBuilderFromInline()

class TestDummyInlineBuilderFromBlock:
    def __init__(self, dummy_text: str):
        self.dummy_text = dummy_text
    def build_from_blocks(self, contents):
        return Text(self.dummy_text)
"#;

#[derive(Debug, PartialEq, Eq)]
pub struct TestDocSegment {
    pub header: Option<(i64, Option<TestBlock>, Option<TestInline>)>,
    pub contents: TestBlock,
    pub subsegments: Vec<TestDocSegment>,
}
#[derive(Debug, PartialEq, Eq)]
pub enum TestBlock {
    BlockScope(Vec<TestBlock>),
    Paragraph(Vec<Vec<TestInline>>),

    /// Test-only - a Python object build from a block scope with test_block: BlockScope = the contents of that scope
    TestOwnedBlock(Vec<TestBlock>),
}
#[derive(Debug, PartialEq, Eq)]
pub enum TestInline {
    InlineScope(Vec<TestInline>),
    Text(String),
    Raw(String),

    /// Test-only - a Python object built from an inline scope with test_inline: InlineScope = the contents of that scope
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
                    let block_contents = match header.as_ref(py).getattr("test_block") {
                        Ok(test_block) => {
                            if test_block.is_none() {
                                None
                            } else {
                                Some(test_block.as_test(py))
                            }
                        }
                        Err(_) => None,
                    };
                    let inline_contents = match header.as_ref(py).getattr("test_inline") {
                        Ok(test_inline) => {
                            if test_inline.is_none() {
                                None
                            } else {
                                Some(test_inline.as_test(py))
                            }
                        }
                        Err(_) => None,
                    };
                    (weight, block_contents, inline_contents)
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
