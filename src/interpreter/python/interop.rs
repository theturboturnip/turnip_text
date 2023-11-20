use pyo3::{
    exceptions::{PyTypeError, PyValueError},
    intern,
    prelude::*,
    types::{PyDict, PyFloat, PyIterator, PyList, PyLong, PyString},
};

use crate::{error::TurnipTextError, parser::TurnipTextParser};

use super::typeclass::{PyInstanceList, PyTcRef, PyTcUnionRef, PyTypeclass, PyTypeclassList};

mod error {
    use pyo3::create_exception;

    create_exception!(_native, TurnipTextError, pyo3::exceptions::PyException);
}

#[pymodule]
#[pyo3(name = "_native")]
pub fn turnip_text(py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse_file, m)?)?;
    m.add_function(wrap_pyfunction!(coerce_to_inline, m)?)?;
    m.add_function(wrap_pyfunction!(coerce_to_inline_scope, m)?)?;
    m.add_function(wrap_pyfunction!(coerce_to_block, m)?)?;
    m.add_function(wrap_pyfunction!(coerce_to_block_scope, m)?)?;

    // Primitives
    m.add_class::<UnescapedText>()?;
    m.add_class::<RawText>()?;
    m.add_class::<Sentence>()?;
    m.add_class::<Paragraph>()?;
    m.add_class::<BlockScope>()?;
    m.add_class::<InlineScope>()?;
    m.add_class::<DocSegment>()?;
    // TODO python typehints for this
    m.add_class::<InsertedFile>()?;

    m.add("TurnipTextError", py.get_type::<error::TurnipTextError>())?;

    Ok(())
}

impl TurnipTextError {
    fn to_pyerr(self) -> PyErr {
        self.display_cli_feedback();
        error::TurnipTextError::new_err(format!("{self}"))
    }
}

#[pyfunction]
fn parse_file<'py>(
    py: Python<'py>,
    file: InsertedFile,
    py_env: &PyDict,
) -> PyResult<Py<DocSegment>> {
    let parser =
        TurnipTextParser::new(py, file.name, file.contents).map_err(TurnipTextError::to_pyerr)?;
    parser.parse(py, py_env).map_err(TurnipTextError::to_pyerr)
}

#[pyfunction]
pub fn coerce_to_inline<'py>(py: Python<'py>, obj: &'py PyAny) -> PyResult<PyObject> {
    Ok(coerce_to_inline_pytcref(py, obj)?.unbox())
}

pub fn coerce_to_inline_pytcref<'py>(
    py: Python<'py>,
    obj: &'py PyAny,
) -> PyResult<PyTcRef<Inline>> {
    // 1. if it's already Inline, return it
    if let Ok(inl) = PyTcRef::of(obj) {
        return Ok(inl);
    }
    // 2. if it's a List of Inline, return InlineScope(it)
    // Here we first check if it's a list, then if so try to create an InlineScope - this will verify if it's a list of Inlines.
    if let Ok(py_list) = obj.downcast::<PyList>() {
        if let Ok(inline_scope) = InlineScope::new(py, Some(py_list)) {
            let inline_scope = Py::new(py, inline_scope)?;
            return PyTcRef::of(inline_scope.as_ref(py));
        }
    }
    // 3. if it's str, return UnescapedText(it)
    if let Ok(py_str) = obj.downcast::<PyString>() {
        let unescaped_text = Py::new(py, UnescapedText::new(py_str))?;
        return PyTcRef::of(unescaped_text.as_ref(py));
    }
    // 4. if it's float, return UnescapedText(str(it))
    // 5. if it's int, return UnescapedText(str(it))
    if obj.downcast::<PyFloat>().is_ok() || obj.downcast::<PyLong>().is_ok() {
        let str_of_obj = obj.str()?;
        let unescaped_text = Py::new(py, UnescapedText::new(str_of_obj))?;
        return PyTcRef::of(unescaped_text.as_ref(py));
    }
    // 6. otherwise fail with TypeError
    Err(PyTypeError::new_err("Failed to coerce object to Inline: was not an Inline, list of Inline (coercible to InlineScope), str, float, or int."))
}

#[pyfunction]
pub fn coerce_to_inline_scope<'py>(py: Python<'py>, obj: &'py PyAny) -> PyResult<Py<InlineScope>> {
    // 1. if it's already InlineScope, return it
    if let Ok(inline_scope) = obj.extract() {
        return Ok(inline_scope);
    }
    // 2. attempt coercion to inline, if it fails return typeerror
    let obj = coerce_to_inline(py, obj)?;
    // 3. if the coercion produced InlineScope, return that
    if let Ok(inline_scope) = obj.extract(py) {
        return Ok(inline_scope);
    }
    // 4. otherwise return InlineScope([that])
    return Ok(Py::new(
        py,
        InlineScope::new(py, Some(PyList::new(py, [obj])))?,
    )?);
}

#[pyfunction]
pub fn coerce_to_block<'py>(py: Python<'py>, obj: &'py PyAny) -> PyResult<PyObject> {
    Ok(coerce_to_block_pytcref(py, obj)?.unbox())
}

pub fn coerce_to_block_pytcref<'py>(py: Python<'py>, obj: &'py PyAny) -> PyResult<PyTcRef<Block>> {
    // 1. if it's already Block, return it
    if let Ok(block) = PyTcRef::of(obj) {
        return Ok(block);
    }
    // 2. if it's a list of Block, wrap it in a BlockScope and return it
    // Here we first check if it's a list, then if so try to create a BlockScope - this will verify if it's a list of Blocks.
    if let Ok(py_list) = obj.downcast::<PyList>() {
        if let Ok(block_scope) = BlockScope::new(py, Some(py_list)) {
            let block_scope = Py::new(py, block_scope)?;
            return PyTcRef::of(block_scope.as_ref(py));
        }
    }
    // 3. if it's a Sentence, wrap it in a list -> Paragraph
    if let Ok(sentence) = obj.extract::<Py<Sentence>>() {
        let paragraph = Py::new(
            py,
            Paragraph::new(py, Some(PyList::new(py, [sentence]).into()))?,
        )?;
        return PyTcRef::of(paragraph.as_ref(py));
    }
    // 4. if it can be coerced to an Inline, wrap that in list -> Sentence -> list -> Paragraph and return it
    if let Ok(inl) = coerce_to_inline(py, obj) {
        let paragraph = Py::new(
            py,
            Paragraph::new(
                py,
                Some(PyList::new(
                    py,
                    [Py::new(
                        py,
                        Sentence::new(py, Some(PyList::new(py, [inl])))?,
                    )?],
                )),
            )?,
        )?;
        return PyTcRef::of(paragraph.as_ref(py));
    }
    // 5. otherwise fail with TypeError
    Err(PyTypeError::new_err("Failed to coerce object to Block: was not a Block, list of Blocks (coercible to BlockScope), Paragraph, Sentence, or coercible to Inline."))
}

#[pyfunction]
pub fn coerce_to_block_scope<'py>(py: Python<'py>, obj: &'py PyAny) -> PyResult<Py<BlockScope>> {
    // 1. if it's already a BlockScope, return it
    if let Ok(block_scope) = obj.extract() {
        return Ok(block_scope);
    }
    // 2. attempt coercion to block, if it fails return typeerror
    let obj = coerce_to_block(py, obj)?;
    // 3. if the coercion produced BlockScope, return that
    if let Ok(block_scope) = obj.extract(py) {
        return Ok(block_scope);
    }
    // 4. otherwise return BlockScope([that])
    return Ok(Py::new(
        py,
        BlockScope::new(py, Some(PyList::new(py, [obj])))?,
    )?);
}

/// Typeclass for block elements within the document tree e.g. paragraphs, block scopes.
#[derive(Debug, Clone)]
pub struct Block {}
impl Block {
    fn marker_bool_name(py: Python<'_>) -> &PyString {
        intern!(py, "is_block")
    }
}
impl PyTypeclass for Block {
    const NAME: &'static str = "Block";

    fn fits_typeclass(obj: &PyAny) -> PyResult<bool> {
        let attr_name = Self::marker_bool_name(obj.py());
        if matches!(obj.hasattr(attr_name), Ok(true)) {
            obj.getattr(attr_name)?.is_true()
        } else {
            Ok(false)
        }
    }
}

/// Typeclass for objects representing content that stays within a single line/sentence.
/// Captures everything that is *not* a block.
#[derive(Debug, Clone)]
pub struct Inline {}
impl Inline {
    fn marker_bool_name(py: Python<'_>) -> &PyString {
        intern!(py, "is_inline")
    }
}
impl PyTypeclass for Inline {
    const NAME: &'static str = "Inline";

    fn fits_typeclass(obj: &PyAny) -> PyResult<bool> {
        let attr_name = Self::marker_bool_name(obj.py());
        if matches!(obj.hasattr(attr_name), Ok(true)) {
            obj.getattr(attr_name)?.is_true()
        } else {
            Ok(false)
        }
    }
}

#[derive(Debug, Clone)]
pub struct DocSegmentHeader {}
impl DocSegmentHeader {
    fn marker_bool_name(py: Python<'_>) -> &PyString {
        intern!(py, "is_segment_header")
    }
    fn weight_field_name(py: Python<'_>) -> &PyString {
        intern!(py, "weight")
    }
    pub fn get_weight(py: Python<'_>, header: &PyAny) -> PyResult<i64> {
        header
            .getattr(DocSegmentHeader::weight_field_name(py))?
            .extract()
    }
}
impl PyTypeclass for DocSegmentHeader {
    const NAME: &'static str = "DocSegmentHeader";

    fn fits_typeclass(obj: &PyAny) -> PyResult<bool> {
        let attr_name = Self::marker_bool_name(obj.py());
        if matches!(obj.hasattr(attr_name), Ok(true)) {
            obj.getattr(attr_name)?.is_true()
        } else {
            Ok(false)
        }
    }
}

/// Typeclass representing the "builder" of a block scope, which may modify how that scope is rendered.
///
/// Requires a method
/// ```python
/// def build_from_blocks(self, blocks: BlockScope) -> Block: ...
/// ```
#[derive(Debug, Clone)]
pub struct BlockScopeBuilder {}
impl BlockScopeBuilder {
    fn marker_func_name(py: Python<'_>) -> &PyString {
        intern!(py, "build_from_blocks")
    }
    pub fn call_build_from_blocks<'py>(
        py: Python<'py>,
        builder: PyTcRef<Self>,
        blocks: Py<BlockScope>,
    ) -> PyResult<Option<PyTcUnionRef<Block, DocSegmentHeader>>> {
        let output = builder
            .as_ref(py)
            .getattr(Self::marker_func_name(py))?
            .call1((blocks,))?;
        if output.is_none() {
            Ok(None)
        } else {
            Ok(Some(PyTcUnionRef::of(output)?))
        }
    }
}
impl PyTypeclass for BlockScopeBuilder {
    const NAME: &'static str = "BlockScopeBuilder";

    fn fits_typeclass(obj: &PyAny) -> PyResult<bool> {
        obj.hasattr(Self::marker_func_name(obj.py()))
    }
}

/// Typeclass representing the "builder" of an inline scope, which may modify how that scope is rendered.
///
/// Requires a method
/// ```python
/// def build_from_inlines(self, inlines: InlineScope) -> Inline: ...
/// ```
#[derive(Debug, Clone)]
pub struct InlineScopeBuilder {}
impl InlineScopeBuilder {
    fn marker_func_name(py: Python<'_>) -> &PyString {
        intern!(py, "build_from_inlines")
    }
    // TODO: Make this return PyTcUnionRef<Inline | DocSegmentHeader>. Right now it can't because the parser can't handle it.
    pub fn call_build_from_inlines<'py>(
        py: Python<'py>,
        builder: PyTcRef<Self>,
        inlines: Py<InlineScope>,
    ) -> PyResult<PyTcRef<Inline>> {
        let output = builder
            .as_ref(py)
            .getattr(Self::marker_func_name(py))?
            .call1((inlines,))?;
        PyTcRef::of(output)
    }
}
impl PyTypeclass for InlineScopeBuilder {
    const NAME: &'static str = "InlineScopeBuilder";

    fn fits_typeclass(obj: &PyAny) -> PyResult<bool> {
        obj.hasattr(Self::marker_func_name(obj.py()))
    }
}

/// Typeclass representing the "builder" of a raw scope, which interprets how that scope is rendered.
///
/// Requires a method
/// ```python
/// def build_from_raw(self, raw: str) -> Inline: ...
/// ```
#[derive(Debug, Clone)]
pub struct RawScopeBuilder {}
impl RawScopeBuilder {
    fn marker_func_name(py: Python<'_>) -> &PyString {
        intern!(py, "build_from_raw")
    }
    /// Calls builder.build_from_raw(raw), could be inline or block
    pub fn call_build_from_raw<'py>(
        py: Python<'py>,
        builder: &PyTcRef<Self>,
        raw: &String,
    ) -> PyResult<PyTcUnionRef<Inline, Block>> {
        let output = builder
            .as_ref(py)
            .getattr(Self::marker_func_name(py))?
            .call1((raw,))?;
        PyTcUnionRef::of(output)
    }
}
impl PyTypeclass for RawScopeBuilder {
    const NAME: &'static str = "RawScopeBuilder";

    fn fits_typeclass(obj: &PyAny) -> PyResult<bool> {
        obj.hasattr(Self::marker_func_name(obj.py()))
    }
}

// /// Typeclass representing the "builder" of a document segment header.
// ///
// /// Requires a method
// /// ```python
// /// def build_doc_segment_header(self, contents: BlockScope) -> DocSegmentHeader: ...
// /// ```
// #[derive(Debug, Clone)]
// pub struct DocSegmentBuilder {}
// impl DocSegmentBuilder {
//     fn marker_func_name(py: Python<'_>) -> &PyString {
//         intern!(py, "build_doc_segment")
//     }
//     /// Calls builder.build_doc_segment_header(contents), should return DocSegmentHeader
//     pub fn call_build_doc_segment<'py>(
//         py: Python<'py>,
//         builder: &PyTcRef<Self>,
//         header: Py<BlockScope>,
//     ) -> PyResult<(PyTcRef<DocSegmentHeader>, i64)> {
//         let output = builder
//             .as_ref(py)
//             .getattr(Self::marker_func_name(py))?
//             .call1((header,))?;
//         Ok((PyTcRef::of(output)?, DocSegmentHeader::get_weight(py, output)?))
//     }
// }
// impl PyTypeclass for DocSegmentBuilder {
//     const NAME: &'static str = "DocSegmentBuilder";

//     fn fits_typeclass(obj: &PyAny) -> PyResult<bool> {
//         obj.hasattr(Self::marker_func_name(obj.py()))
//     }
// }

/// Represents plain inline text that has not yet been "escaped" for rendering.
///
/// Typically created by Rust while parsing input files.
#[pyclass]
#[derive(Debug, Clone)]
pub struct UnescapedText(pub Py<PyString>);
impl UnescapedText {
    pub fn new_rs(py: Python, s: &str) -> Self {
        Self::new(PyString::new(py, s))
    }
}
#[pymethods]
impl UnescapedText {
    #[new]
    pub fn new(data: &PyString) -> Self {
        Self(data.into())
    }
    #[getter]
    pub fn text(&self) -> PyResult<Py<PyString>> {
        Ok(self.0.clone())
    }
    #[getter]
    pub fn is_inline(&self) -> bool {
        true
    }
}

/// Represents raw text that should not be escaped for rendering.
///
/// Typically created by Rust while parsing input files.
#[pyclass]
#[derive(Debug, Clone)]
pub struct RawText(pub Py<PyString>);
impl RawText {
    pub fn new_rs(py: Python, s: &str) -> Self {
        Self::new(PyString::new(py, s).into_py(py))
    }
}
#[pymethods]
impl RawText {
    #[new]
    pub fn new(data: Py<PyString>) -> Self {
        Self(data)
    }
    #[getter]
    pub fn text(&self) -> PyResult<Py<PyString>> {
        Ok(self.0.clone())
    }
    #[getter]
    pub fn is_inline(&self) -> bool {
        true
    }
}

/// A sequence of objects that represents a single sentence.
///
/// Typically created by Rust while parsing input files.
#[pyclass(sequence)]
#[derive(Debug, Clone)]
pub struct Sentence(pub PyTypeclassList<Inline>);
impl Sentence {
    pub fn new_empty(py: Python) -> Self {
        Self(PyTypeclassList::new(py))
    }
}
#[pymethods]
impl Sentence {
    #[new]
    #[pyo3(signature = (list=None))]
    pub fn new(py: Python, list: Option<&PyList>) -> PyResult<Self> {
        match list {
            Some(list) => Ok(Self(PyTypeclassList::from(list)?)),
            None => Ok(Self(PyTypeclassList::new(py))),
        }
    }

    pub fn __len__(&self, py: Python) -> usize {
        self.0.list(py).len()
    }
    pub fn __iter__<'py>(&'py self, py: Python<'py>) -> PyResult<&'py PyIterator> {
        PyIterator::from_object(py, self.0.list(py))
    }

    pub fn push_inline(&mut self, node: &PyAny) -> PyResult<()> {
        self.0.append_checked(node)
    }
}

/// A sequence of [Sentence] that combine to make a complete paragraph.
///
/// Typically created by Rust while parsing input files.
#[pyclass(sequence)]
#[derive(Debug, Clone)]
pub struct Paragraph(pub PyInstanceList<Sentence>);
impl Paragraph {
    pub fn new_empty(py: Python) -> Self {
        Self(PyInstanceList::new(py))
    }
}
#[pymethods]
impl Paragraph {
    #[new]
    #[pyo3(signature = (list=None))]
    pub fn new(py: Python, list: Option<&PyList>) -> PyResult<Self> {
        match list {
            Some(list) => Ok(Self(PyInstanceList::from(list)?)),
            None => Ok(Self(PyInstanceList::new(py))),
        }
    }

    #[getter]
    pub fn is_block(&self) -> bool {
        true
    }

    pub fn __len__(&self, py: Python) -> usize {
        self.0.list(py).len()
    }
    pub fn __iter__<'py>(&'py self, py: Python<'py>) -> PyResult<&'py PyIterator> {
        PyIterator::from_object(py, self.0.list(py))
    }

    pub fn push_sentence(&mut self, node: &PyAny) -> PyResult<()> {
        self.0.append_checked(node)
    }
}

/// A group of [Block]s inside non-code-preceded squiggly braces
///
/// Typically created by Rust while parsing input files.
#[pyclass(sequence)]
#[derive(Debug, Clone)]
pub struct BlockScope(pub PyTypeclassList<Block>);
impl BlockScope {
    pub fn new_empty(py: Python) -> Self {
        Self(PyTypeclassList::new(py))
    }
}
#[pymethods]
impl BlockScope {
    #[new]
    #[pyo3(signature = (list=None))]
    pub fn new(py: Python, list: Option<&PyList>) -> PyResult<Self> {
        match list {
            Some(list) => Ok(Self(PyTypeclassList::from(list)?)),
            None => Ok(Self(PyTypeclassList::new(py))),
        }
    }

    #[getter]
    pub fn is_block(&self) -> bool {
        true
    }

    pub fn __len__(&self, py: Python) -> usize {
        self.0.list(py).len()
    }
    pub fn __iter__<'py>(&'py self, py: Python<'py>) -> PyResult<&'py PyIterator> {
        PyIterator::from_object(py, self.0.list(py))
    }

    pub fn push_block(&mut self, node: &PyAny) -> PyResult<()> {
        self.0.append_checked(node)
    }
}

/// A group of [Inline]s inside non-code-preceded squiggly braces
///
/// Typically created by Rust while parsing input files.
#[pyclass(sequence)]
#[derive(Debug, Clone)]
pub struct InlineScope(pub PyTypeclassList<Inline>);
impl InlineScope {
    pub fn new_empty(py: Python) -> Self {
        Self(PyTypeclassList::new(py))
    }
}
#[pymethods]
impl InlineScope {
    #[new]
    #[pyo3(signature = (list=None))]
    pub fn new(py: Python, list: Option<&PyList>) -> PyResult<Self> {
        match list {
            Some(list) => Ok(Self(PyTypeclassList::from(list)?)),
            None => Ok(Self(PyTypeclassList::new(py))),
        }
    }

    #[getter]
    pub fn is_inline(&self) -> bool {
        true
    }

    pub fn __len__(&self, py: Python) -> usize {
        self.0.list(py).len()
    }
    pub fn __iter__<'py>(&'py self, py: Python<'py>) -> PyResult<&'py PyIterator> {
        PyIterator::from_object(py, self.0.list(py))
    }

    pub fn push_inline(&mut self, node: &PyAny) -> PyResult<()> {
        self.0.append_checked(node)
    }
}

#[pyclass]
#[derive(Debug, Clone)]
pub struct InsertedFile {
    pub name: String,
    pub contents: String,
}
impl InsertedFile {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn contents(&self) -> &str {
        &self.contents
    }
}
#[pymethods]
impl InsertedFile {
    #[new]
    pub fn new(name: String, contents: String) -> InsertedFile {
        InsertedFile { name, contents }
    }

    #[staticmethod]
    pub fn from_path(path: String) -> PyResult<InsertedFile> {
        let name = std::fs::canonicalize(&path)?;
        let contents = std::fs::read_to_string(&path)
            .map_err(|e| error::TurnipTextError::new_err(format!("{e}")))?;
        Ok(InsertedFile {
            name: name.into_os_string().into_string().unwrap_or(path),
            contents,
        })
    }

    #[staticmethod]
    pub fn from_string(contents: String) -> InsertedFile {
        InsertedFile {
            name: "<string>".into(),
            contents,
        }
    }
}

/// This is used for implicit structure.
/// It's created by Python code just with the Header and Weight, and the Weight is used to implicitly open and close scopes
///
/// ```text
/// [heading("Blah")] # -> DocSegmentBuilder(weight=0)
///
/// # This paragraph is implicitly included as a child of DocSegment().text
/// some text
///
/// [subheading("Sub-Blah")] # -> DocSegmentBuilder(weight=1)
/// # the weight is greater than for the Section -> the subsection is implicitly included as a subsegment of section
///
/// Some other text in a subsection
///
/// [heading("Blah 2")] # -> DocSegmentBuilder(weight=0)
/// # the weight is <=subsection -> subsection 1.1 automatically ends, subheading.build_doc_segment() called
/// # the weight is <=section -> section 1 automatically ends, heading.build_doc_segment() called
///
/// Some other text in a second section
/// ```
///
/// There can be weird interactions with manual scopes.
/// It may be confusing for a renderer to find Section containing Manual Scope containing Subsection,
/// having the Subsection not as a direct child of the Section.
/// Thus we allow manual scopes to be opened and closed as usual, but we don't allow DocSegments *within* them.
/// Effectively DocSegments must only exist at the "top level" - they may be enclosed by DocSegments directly, but nothing else.
/// TODO this means subfiles need to emit lists of blocks directly into the enclosing BlockScope,
/// as otherwise you couldn't have an DocSegments inside them at all - they'd all be implicitly contained by a BlockScope
///
/// An example error from mixing explicit and implicit scoping:
/// ```text
/// [section("One")]
///
/// this text is clearly in section 1
///
/// {
///     [subsection("One.One")]
///
///     this text is clearly in subsection 1.1...
/// } # Maybe this should be an error? but then it's only a problem if there's bare text underneath...
///
/// but where is this?? # This is really the error.
///
/// [subsection("One.Two")]
///
/// this text is clearly in subsection 1.2
/// ```
#[pyclass]
#[derive(Debug, Clone)]
pub struct DocSegment {
    pub header: Option<PyTcRef<DocSegmentHeader>>,
    pub contents: Py<BlockScope>,
    pub subsegments: PyInstanceList<DocSegment>,
}
impl DocSegment {
    pub fn new_no_header(
        py: Python,
        contents: Py<BlockScope>,
        subsegments: PyInstanceList<DocSegment>,
    ) -> PyResult<Self> {
        Self::new_checked(py, None, contents, subsegments)
    }
    pub fn new_checked(
        py: Python,
        header: Option<PyTcRef<DocSegmentHeader>>,
        contents: Py<BlockScope>,
        subsegments: PyInstanceList<DocSegment>,
    ) -> PyResult<Self> {
        match &header {
            Some(h) => {
                let weight = DocSegmentHeader::get_weight(py, h.as_ref(py))?;
                for subsegment in subsegments.list(py).iter() {
                    let subsegment: Py<DocSegment> = subsegment.extract()?;
                    match &subsegment.borrow(py).header {
                        Some(subh) => {
                            let subweight = DocSegmentHeader::get_weight(py, subh.as_ref(py))?;
                            if subweight <= weight {
                                return Err(PyValueError::new_err(format!("Trying to create a DocSegment with weight {weight} but one of the provided subsegments has weight {subweight} which is smaller. Only larger subweights are allowed.")))
                            }
                        }
                        None => return Err(PyValueError::new_err(format!("Trying to create a DocSegment but a subsegement doesn't have a header. All subsegments must have headers.")))
                    };
                }
            }
            None => {}
        }

        Ok(Self {
            header,
            contents,
            subsegments,
        })
    }
}
#[pymethods]
impl DocSegment {
    #[new]
    pub fn new(header: &PyAny, contents: Py<BlockScope>, subsegments: &PyList) -> PyResult<Self> {
        Ok(Self {
            header: Some(PyTcRef::of(header)?),
            contents,
            subsegments: PyInstanceList::from(subsegments)?,
        })
    }
    #[getter]
    pub fn header<'py>(&'py self, py: Python<'py>) -> Option<&'py PyAny> {
        self.header.as_ref().map(|obj| obj.as_ref(py))
    }
    #[getter]
    pub fn contents<'py>(&'py self, py: Python<'py>) -> &'py PyAny {
        self.contents.as_ref(py)
    }
    #[getter]
    pub fn subsegments<'py>(&'py self, py: Python<'py>) -> PyResult<&'py PyIterator> {
        PyIterator::from_object(py, self.subsegments.list(py))
    }
    pub fn push_subsegment(&self, py: Python<'_>, subsegment: Py<DocSegment>) -> PyResult<()> {
        match (&self.header, &subsegment.borrow(py).header) {
            (Some(header), Some(subheader)) => {
                let weight = DocSegmentHeader::get_weight(py, header.as_ref(py))?;
                let subweight = DocSegmentHeader::get_weight(py, subheader.as_ref(py))?;
                if subweight <= weight {
                    return Err(PyValueError::new_err(format!("Trying to add to a DocSegment with weight {weight} but the provided subsegment has weight {subweight} which is smaller. Only larger subweights are allowed.")))
                }
            }
            (_, None) => return Err(PyValueError::new_err(format!("Trying to add to a DocSegment but the subsegement doesn't have a header. All subsegments must have headers."))),
            _ => {}
        };
        self.subsegments.append_checked(subsegment.as_ref(py))
    }
}
