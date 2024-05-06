use crate::interpreter::error::HandleInternalPyErr;
use crate::python::interop::{
    BlockScope, DocSegment, Document, Header, InlineScope, Paragraph, Raw, Sentence, Text,
};
use pyo3::prelude::*;
use pyo3::types::PyDict;

pub const GLOBALS_CODE: &'static str = r#"
# The Rust module name is _native, which is included under turnip_text, so Python IDEs don't try to import directly from it.
# This means we use _native instead of turnip_text as the module name here.
from _native import InlineScope, Text, BlockScope, TurnipTextSource, Paragraph, Sentence, Raw

class CustomHeader:
    is_header = True
    weight = 0
    def __init__(self, weight=0, test_block=None, test_inline=None):
        self.weight = weight
        self.test_block = test_block
        self.test_inline = test_inline

class CustomBlock:
    is_block = True
    def __init__(self, contents):
        self.test_block = contents

class CustomInline:
    is_inline = True
    def __init__(self, contents):
        self.test_inline = contents

class CustomRaw:
    is_inline = True
    def __init__(self, raw_str):
        self.test_raw_str = str(raw_str)

CUSTOM_BLOCK = CustomBlock(BlockScope([]))
CUSTOM_INLINE = CustomInline(InlineScope([]))
CUSTOM_RAW = CustomRaw("")

class CustomBlockBuilder:
    def build_from_blocks(self, contents):
        return CustomBlock(contents)
class CustomBlockBuilderFromInline:
    def build_from_inlines(self, contents: InlineScope):
        return CustomBlock(BlockScope([Paragraph([Sentence([contents])])]))
class CustomBlockBuilderFromRaw:
    def build_from_raw(self, raw):
        return CustomBlock(BlockScope([Paragraph([Sentence([raw])])]))
    
class CustomInlineBuilder:
    def build_from_inlines(self, contents):
        return CustomInline(contents)

class CustomRawBuilder:
    def build_from_raw(self, raw):
        return CustomRaw(raw.data)

BUILD_CUSTOM_BLOCK = CustomBlockBuilder()
BUILD_CUSTOM_BLOCK_FROM_INLINE = CustomBlockBuilderFromInline()
BUILD_CUSTOM_BLOCK_FROM_RAW = CustomBlockBuilderFromRaw()
BUILD_CUSTOM_INLINE = CustomInlineBuilder()
BUILD_CUSTOM_RAW = CustomRawBuilder()

class BlockSwallower():
    def build_from_blocks(self, contents):
        return None
class InlineSwallower():
    def build_from_inlines(self, contents):
        return None
class RawSwallower():
    def build_from_raw(self, raw):
        return None

CUSTOM_BLOCK_SWALLOWER = BlockSwallower()
CUSTOM_INLINE_SWALLOWER = InlineSwallower()
CUSTOM_RAW_SWALLOWER = RawSwallower()

class CustomHeaderBuilder:
    def __init__(self, weight=0):
        self.weight=weight
    def build_from_blocks(self, contents):
        return CustomHeader(weight=self.weight, test_block=contents)
    def build_from_inlines(self, contents):
        return CustomHeader(weight=self.weight, test_inline=contents)
    def build_from_raw(self, raw):
        return CustomHeader(weight=self.weight, test_inline=InlineScope([raw]))

def test_src(contents):
    return TurnipTextSource.from_string(contents)

class TestDummyInlineBuilderFromBlock:
    def __init__(self, dummy_text: str):
        self.dummy_text = dummy_text
    def build_from_blocks(self, contents):
        return Text(self.dummy_text)

# Test code likes to use "1invalid" as a basic example of non-indentation invalid syntax.
# That raises a SyntaxWarning in the test log, which is annoying.
import warnings
warnings.filterwarnings("ignore", category=SyntaxWarning)
"#;
#[derive(Debug, PartialEq, Eq)]
pub struct TestDocument {
    pub contents: TestBlock,
    pub segments: Vec<TestDocSegment>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct TestDocSegment {
    pub header: (i64, Option<TestBlock>, Option<TestInline>),
    pub contents: TestBlock,
    pub subsegments: Vec<TestDocSegment>,
}
#[derive(Debug, PartialEq, Eq)]
pub enum TestBlock {
    BlockScope(Vec<TestBlock>),
    Paragraph(Vec<Vec<TestInline>>),

    /// Test-only - a Python object build from a block scope with test_block: BlockScope = the contents of that scope
    CustomBlock(Vec<TestBlock>),
}
#[derive(Debug, PartialEq, Eq)]
pub enum TestInline {
    InlineScope(Vec<TestInline>),
    Text(String),
    Raw(String),

    /// Test-only - a Python object built from an inline scope with test_inline: InlineScope = the contents of that scope
    CustomInline(Vec<TestInline>),
    /// Test-only - a Python object built from raw text with test_raw_str: str = the raw text
    CustomRaw(String),
}
pub fn test_doc(contents: Vec<TestBlock>) -> TestDocument {
    TestDocument {
        contents: TestBlock::BlockScope(contents),
        segments: vec![],
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
impl PyToTest<TestDocument> for Bound<'_, PyAny> {
    fn as_test(&self, py: Python) -> TestDocument {
        if let Ok(document) = self.extract::<Document>() {
            TestDocument {
                contents: document.contents.bind(py).as_any().as_test(py),
                segments: document
                    .segments
                    .list(py)
                    .iter()
                    .map(|subseg| subseg.as_any().as_test(py))
                    .collect(),
            }
        } else {
            let repr = match self.repr() {
                Ok(py_str) => py_str.to_string(),
                Err(_) => "<couldn't call __repr__>".to_owned(),
            };
            panic!("Expected Document, got {repr}")
        }
    }
}
impl PyToTest<TestDocSegment> for Bound<'_, PyAny> {
    fn as_test(&self, py: Python) -> TestDocSegment {
        if let Ok(doc_segment) = self.extract::<DocSegment>() {
            TestDocSegment {
                header: {
                    let weight = Header::get_weight(py, doc_segment.header.bind(py))
                        .expect("Couldn't get_weight of header");
                    let block_contents = match doc_segment.header.bind(py).getattr("test_block") {
                        Ok(test_block) => {
                            if test_block.is_none() {
                                None
                            } else {
                                Some(test_block.as_any().as_test(py))
                            }
                        }
                        Err(_) => None,
                    };
                    let inline_contents = match doc_segment.header.bind(py).getattr("test_inline") {
                        Ok(test_inline) => {
                            if test_inline.is_none() {
                                None
                            } else {
                                Some(test_inline.as_any().as_test(py))
                            }
                        }
                        Err(_) => None,
                    };
                    (weight, block_contents, inline_contents)
                },
                contents: doc_segment.contents.bind(py).as_any().as_test(py),
                subsegments: doc_segment
                    .subsegments
                    .list(py)
                    .iter()
                    .map(|subseg| subseg.as_any().as_test(py))
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
impl PyToTest<TestBlock> for Bound<'_, PyAny> {
    fn as_test(&self, py: Python) -> TestBlock {
        if let Ok(block) = self.extract::<BlockScope>() {
            TestBlock::BlockScope(
                block
                    .0
                    .list(py)
                    .iter()
                    .map(|obj| PyToTest::as_test(&obj, py))
                    .collect(),
            )
        } else if let Ok(para) = self.extract::<Paragraph>() {
            TestBlock::Paragraph(
                para.0
                    .list(py)
                    .iter()
                    .map(|obj| PyToTest::as_test(&obj, py))
                    .collect(),
            )
        } else if let Ok(obj) = self.getattr("test_block") {
            TestBlock::CustomBlock(
                obj.extract::<BlockScope>()
                    .unwrap()
                    .0
                    .list(py)
                    .iter()
                    .map(|obj| PyToTest::as_test(&obj, py))
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
impl PyToTest<Vec<TestInline>> for Bound<'_, PyAny> {
    fn as_test(&self, py: Python) -> Vec<TestInline> {
        if let Ok(sentence) = self.extract::<Sentence>() {
            sentence
                .0
                .list(py)
                .iter()
                .map(|obj| PyToTest::as_test(&obj, py))
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
impl PyToTest<TestInline> for Bound<'_, PyAny> {
    fn as_test(&self, py: Python) -> TestInline {
        if let Ok(inl) = self.extract::<InlineScope>() {
            TestInline::InlineScope(
                inl.0
                    .list(py)
                    .iter()
                    .map(|obj| PyToTest::as_test(&obj, py))
                    .collect(),
            )
        } else if let Ok(text) = self.extract::<Text>() {
            TestInline::Text(text.0.bind(py).to_string())
        } else if let Ok(text) = self.extract::<Raw>() {
            TestInline::Raw(text.0.bind(py).to_string())
        } else if let Ok(obj) = self.getattr("test_inline") {
            TestInline::CustomInline(
                obj.extract::<InlineScope>()
                    .unwrap()
                    .0
                    .list(py)
                    .iter()
                    .map(|obj| PyToTest::as_test(&obj, py))
                    .collect(),
            )
        } else if let Ok(text) = dbg!(self.getattr("test_raw_str")) {
            TestInline::CustomRaw(text.to_string())
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
/// Provides `BUILD_CUSTOM_BLOCK`, `BUILD_CUSTOM_INLINE`, `CUSTOM_RAW_BUILDER` objects
/// that can own block, inline, and raw scopes respectively.
pub fn generate_globals<'interp>(py: Python<'interp>) -> Bound<'interp, PyDict> {
    let globals = PyDict::new_bound(py);

    py.run_bound(GLOBALS_CODE, Some(&globals), Some(&globals))
        .expect_pyok("generate_globals");

    globals
}

pub fn stringify_pyerr(py: Python, pyerr: &PyErr) -> String {
    let value_bound = pyerr.value_bound(py);
    // let type_bound = pyerr.get_type_bound(py);
    if let Ok(s) = value_bound.str() {
        match value_bound.get_type().qualname() {
            Ok(name) => format!("{0} : {1}", name, &s.to_string_lossy()),
            Err(_) => format!("Unknown Error Type : {}", &s.to_string_lossy()),
        }
    } else {
        "<exception str() failed>".into()
    }
}
