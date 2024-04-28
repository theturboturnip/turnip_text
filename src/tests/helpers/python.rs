use crate::python::interop::{
    BlockScope, DocSegment, DocSegmentHeader, InlineScope, Paragraph, Raw, Sentence, Text,
};
use pyo3::prelude::*;
use pyo3::types::PyDict;

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
