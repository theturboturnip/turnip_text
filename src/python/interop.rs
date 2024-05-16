use std::num::NonZeroUsize;

use pyo3::{
    create_exception,
    exceptions::{PyTypeError, PyValueError},
    intern,
    prelude::*,
    types::{PyDict, PyFloat, PyIterator, PyList, PyLong, PySequence, PyString},
};

use crate::interpreter::{RecursionConfig, TurnipTextParser};

use super::typeclass::{PyInstanceList, PyTcRef, PyTypeclass, PyTypeclassList};

create_exception!(_native, TurnipTextError, pyo3::exceptions::PyException);

#[pymodule]
#[pyo3(name = "_native")]
pub fn turnip_text(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse_file, m)?)?;
    m.add_function(wrap_pyfunction!(coerce_to_inline, m)?)?;
    m.add_function(wrap_pyfunction!(coerce_to_inline_scope, m)?)?;
    m.add_function(wrap_pyfunction!(coerce_to_block, m)?)?;
    m.add_function(wrap_pyfunction!(coerce_to_blocks, m)?)?;

    // Primitives
    m.add_class::<Text>()?;
    m.add_class::<Raw>()?;
    m.add_class::<Sentence>()?;
    m.add_class::<Paragraph>()?;
    m.add_class::<Blocks>()?;
    m.add_class::<InlineScope>()?;
    m.add_class::<Document>()?;
    m.add_class::<DocSegment>()?;
    m.add_class::<TurnipTextSource>()?;

    m.add("TurnipTextError", py.get_type_bound::<TurnipTextError>())?;

    Ok(())
}

/// Implement a Default recursion config that matches the Python defaults
impl Default for RecursionConfig {
    fn default() -> Self {
        Self {
            recursion_warning: true,
            max_file_depth: NonZeroUsize::new(128),
        }
    }
}

#[pyfunction(signature=(file, py_env, recursion_warning=true, max_file_depth=128))]
fn parse_file<'py>(
    py: Python<'py>,
    file: TurnipTextSource,
    py_env: &Bound<'_, PyDict>,
    recursion_warning: bool,
    max_file_depth: usize,
) -> PyResult<Py<Document>> {
    let recursion_config = RecursionConfig {
        recursion_warning,
        max_file_depth: NonZeroUsize::new(max_file_depth),
    };
    match TurnipTextParser::oneshot_parse(py, py_env, file, recursion_config) {
        Ok(doc) => Ok(doc),
        Err(tterr) => Err(tterr.to_pyerr(py)),
    }
}

#[pyfunction]
pub fn coerce_to_inline<'py>(py: Python<'py>, obj: &Bound<'py, PyAny>) -> PyResult<PyObject> {
    Ok(coerce_to_inline_pytcref(py, obj)?.into_any())
}

// FUTURE separate the failure condition of the coercion from other potential failures e.g. allocation failure.
// if let Ok(x) = ? is an antipattern afaik.
pub fn coerce_to_inline_pytcref<'py>(
    py: Python<'py>,
    obj: &Bound<'py, PyAny>,
) -> PyResult<PyTcRef<Inline>> {
    // 1. if it's already Inline, return it
    if let Ok(inl) = PyTcRef::of(obj) {
        return Ok(inl);
    }
    // 2. if it's str, return Text(it)
    // Do this before checking sequence-ness because str is a sequence of str.
    if let Ok(py_str) = obj.downcast::<PyString>() {
        let unescaped_text = Py::new(py, Text::new(py_str))?;
        return Ok(PyTcRef::of_unchecked(unescaped_text.bind(py)));
    }
    // 3. if it's an Sequence of Inline, return InlineScope(it)
    // Here we first check if it's sequence, then if so try to create an InlineScope - this will verify if it's a list of Inlines.
    if let Ok(seq) = obj.downcast::<PySequence>() {
        if let Ok(inline_scope) = InlineScope::new(py, Some(&seq)) {
            let inline_scope = Py::new(py, inline_scope)?;
            return Ok(PyTcRef::of_unchecked(inline_scope.bind(py)));
        }
    }
    // 4. if it's float, return Text(str(it))
    // 5. if it's int, return Text(str(it))
    if obj.downcast::<PyFloat>().is_ok() || obj.downcast::<PyLong>().is_ok() {
        let str_of_obj = obj.str()?;
        let unescaped_text = Py::new(py, Text::new(&str_of_obj))?;
        return Ok(PyTcRef::of_unchecked(unescaped_text.bind(py)));
    }
    // 6. otherwise fail with TypeError
    Err(PyTypeError::new_err(
        "Failed to coerce object to Inline: was not an Inline, list of Inline (coercible to \
         InlineScope), str, float, or int.",
    ))
}

#[pyfunction]
pub fn coerce_to_inline_scope<'py>(
    py: Python<'py>,
    obj: &Bound<'py, PyAny>,
) -> PyResult<Py<InlineScope>> {
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
        InlineScope::new(py, Some(&PyList::new_bound(py, [obj]).as_sequence()))?,
    )?);
}

#[pyfunction]
pub fn coerce_to_block<'py>(py: Python<'py>, obj: &Bound<'py, PyAny>) -> PyResult<PyObject> {
    Ok(coerce_to_block_pytcref(py, obj)?.into_any())
}

pub fn coerce_to_block_pytcref<'py>(
    py: Python<'py>,
    obj: &Bound<'py, PyAny>,
) -> PyResult<PyTcRef<Block>> {
    // 1. if it's already Block, return it
    if let Ok(block) = PyTcRef::of(obj) {
        return Ok(block);
    }
    // 2. if it's a Sentence, wrap it in a list -> Paragraph
    // Do this before checking if it's a sequence, because Sentence is a sequence of InlineScope
    if let Ok(sentence) = obj.extract::<Py<Sentence>>() {
        let paragraph = Py::new(
            py,
            Paragraph::new(py, Some(&PyList::new_bound(py, [sentence]).as_sequence()))?,
        )?;
        return Ok(PyTcRef::of_unchecked(paragraph.bind(py)));
    }
    // 3. if it's an sequence of Block, wrap it in a Blocks and return it
    // Here we first check if it's sequence, then if so try to create a Blocks - this will verify if it's a list of Blocks.
    if let Ok(seq) = obj.downcast::<PySequence>() {
        if let Ok(blocks) = Blocks::new(py, Some(&seq)) {
            let blocks = Py::new(py, blocks)?;
            return Ok(PyTcRef::of_unchecked(blocks.bind(py)));
        }
    }
    // 4. if it can be coerced to an Inline, wrap that in list -> Sentence -> list -> Paragraph and return it
    if let Ok(inl) = coerce_to_inline(py, obj) {
        let paragraph = Py::new(
            py,
            Paragraph::new(
                py,
                Some(
                    &PyList::new_bound(
                        py,
                        [Py::new(
                            py,
                            Sentence::new(py, Some(&PyList::new_bound(py, [inl]).as_sequence()))?,
                        )?],
                    )
                    .as_sequence(),
                ),
            )?,
        )?;
        return Ok(PyTcRef::of_unchecked(paragraph.bind(py)));
    }
    // 5. otherwise fail with TypeError
    Err(PyTypeError::new_err(
        "Failed to coerce object to Block: was not a Block, list of Blocks (coercible to \
         Blocks), Paragraph, Sentence, or coercible to Inline.",
    ))
}

#[pyfunction]
pub fn coerce_to_blocks<'py>(py: Python<'py>, obj: &Bound<'py, PyAny>) -> PyResult<Py<Blocks>> {
    // 1. if it's already a Blocks, return it
    if let Ok(blocks) = obj.extract() {
        return Ok(blocks);
    }
    // 2. attempt coercion to block, if it fails return typeerror
    let obj = coerce_to_block(py, obj)?;
    // 3. if the coercion produced Blocks, return that
    if let Ok(blocks) = obj.extract(py) {
        return Ok(blocks);
    }
    // 4. otherwise return Blocks([that])
    return Ok(Py::new(
        py,
        Blocks::new(py, Some(&PyList::new_bound(py, [obj]).as_sequence()))?,
    )?);
}

/// Typeclass for block elements within the document tree e.g. paragraphs, block scopes.
#[derive(Debug, Clone)]
pub struct Block {}
impl Block {
    fn marker_bool_name(py: Python<'_>) -> &Bound<'_, PyString> {
        intern!(py, "is_block")
    }
}
impl PyTypeclass for Block {
    const NAME: &'static str = "Block";

    fn fits_typeclass(obj: &Bound<'_, PyAny>) -> PyResult<bool> {
        let attr_name = Self::marker_bool_name(obj.py());
        if matches!(obj.hasattr(attr_name), Ok(true)) {
            obj.getattr(attr_name)?.is_truthy()
        } else {
            Ok(false)
        }
    }

    fn get_typeclass_err(obj: &Bound<'_, PyAny>, context: &str) -> PyResult<Option<PyErr>> {
        if Self::fits_typeclass(obj)? {
            Ok(None)
        } else {
            let obj_repr = obj.repr()?;
            let err = PyTypeError::new_err(format!(
                "Expected {} to be an instance of {}, but it didn't have property is_block=True. Got {}",
                context,
                Self::NAME,
                obj_repr.to_str()?
            ));
            Ok(Some(err))
        }
    }
}

/// Typeclass for objects representing content that stays within a single line/sentence.
/// Captures everything that is *not* a block.
#[derive(Debug, Clone)]
pub struct Inline {}
impl Inline {
    fn marker_bool_name(py: Python<'_>) -> &Bound<'_, PyString> {
        intern!(py, "is_inline")
    }
}
impl PyTypeclass for Inline {
    const NAME: &'static str = "Inline";

    fn fits_typeclass(obj: &Bound<'_, PyAny>) -> PyResult<bool> {
        let attr_name = Self::marker_bool_name(obj.py());
        if matches!(obj.hasattr(attr_name), Ok(true)) {
            obj.getattr(attr_name)?.is_truthy()
        } else {
            Ok(false)
        }
    }
    fn get_typeclass_err(obj: &Bound<'_, PyAny>, context: &str) -> PyResult<Option<PyErr>> {
        if Self::fits_typeclass(obj)? {
            Ok(None)
        } else {
            let obj_repr = obj.repr()?;
            let err = PyTypeError::new_err(format!(
                "Expected {} to be an instance of {}, but it didn't have property is_inline=True. Got {}",
                context,
                Self::NAME,
                obj_repr.to_str()?
            ));
            Ok(Some(err))
        }
    }
}

/// Typeclass representing the header for a document segment, which is rendered before any other element in the segment
/// and defines how it interacts with other segments through the weight parameter.
///
/// TODO weight => depth everywhere
/// TODO change this explanation
/// This is used for implicit structure.
/// It's created by Python code by emitting a Header with some Weight, and the Weight is used to implicitly open and close scopes
///
/// ```text
/// [heading("Blah")] # -> Header(weight=0)
///
/// # This paragraph is implicitly included as a child of DocSegment().text
/// some text
///
/// [subheading("Sub-Blah")] # -> Header(weight=1)
/// # the weight is greater than for the Section -> the subsection is implicitly included as a subsegment of section
///
/// Some other text in a subsection
///
/// [heading("Blah 2")] # -> Header(weight=0)
/// # the weight is <=subsection -> subsection 1.1 automatically ends, subheading.build_doc_segment() called
/// # the weight is <=section -> section 1 automatically ends, heading.build_doc_segment() called
///
/// Some other text in a second section
/// ```
///
/// There can be weird interactions with manual scopes.
/// It may be confusing for a renderer to find Section containing Manual Scope containing Subsection,
/// having the Subsection not as a direct child of the Section.
/// Thus we allow manual scopes to be opened and closed as usual, but we don't allow Headers *within* them.
/// Effectively Headers must only exist at the "top level" - they may be enclosed by Headers directly, but nothing else.
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
#[derive(Debug, Clone)]
pub struct Header {}
impl Header {
    fn marker_bool_name(py: Python<'_>) -> &Bound<'_, PyString> {
        intern!(py, "is_header")
    }
    fn weight_field_name(py: Python<'_>) -> &Bound<'_, PyString> {
        intern!(py, "weight")
    }
    pub fn get_weight(py: Python<'_>, header: &Bound<'_, PyAny>) -> PyResult<i64> {
        header.getattr(Header::weight_field_name(py))?.extract()
    }
}
impl PyTypeclass for Header {
    const NAME: &'static str = "Header";

    fn fits_typeclass(obj: &Bound<'_, PyAny>) -> PyResult<bool> {
        let is_header = {
            let attr_name = Self::marker_bool_name(obj.py());
            if matches!(obj.hasattr(attr_name), Ok(true)) {
                obj.getattr(attr_name)?.is_truthy()?
            } else {
                false
            }
        };
        let has_weight_i64 = {
            let attr_name = Self::weight_field_name(obj.py());
            if matches!(obj.hasattr(Self::weight_field_name(obj.py())), Ok(true)) {
                obj.getattr(attr_name)?.extract::<i64>().is_ok()
            } else {
                false
            }
        };

        Ok(is_header && has_weight_i64)
    }
    fn get_typeclass_err(obj: &Bound<'_, PyAny>, context: &str) -> PyResult<Option<PyErr>> {
        let is_header = {
            let attr_name = Self::marker_bool_name(obj.py());
            if matches!(obj.hasattr(attr_name), Ok(true)) {
                obj.getattr(attr_name)?.is_truthy()?
            } else {
                false
            }
        };
        let has_weight_i64 = {
            let attr_name = Self::weight_field_name(obj.py());
            if matches!(obj.hasattr(Self::weight_field_name(obj.py())), Ok(true)) {
                obj.getattr(attr_name)?.extract::<i64>().is_ok()
            } else {
                false
            }
        };

        match (is_header, has_weight_i64) {
            (true, true) => Ok(None),
            (false, false) => {
                let obj_repr = obj.repr()?;
                let err = PyTypeError::new_err(format!(
                    "Expected {} to be an instance of {}, but it didn't have the properties `is_header=True` or `weight: int`. Got {}",
                    context,
                    Self::NAME,
                    obj_repr.to_str()?
                ));
                Ok(Some(err))
            }
            (true, false) => {
                let obj_repr = obj.repr()?;
                let err = PyTypeError::new_err(format!(
                    "Expected {} to be an instance of {}, and it had `is_header=True`, but it didn't have the property `weight: int` (must fit into 64-bit signed int). Got {}",
                    context,
                    Self::NAME,
                    obj_repr.to_str()?
                ));
                Ok(Some(err))
            }
            (false, true) => {
                let obj_repr = obj.repr()?;
                let err = PyTypeError::new_err(format!(
                    "Expected {} to be an instance of {}, and it had the property `weight: int`, but it didn't have the property `is_header=True`. Got {}",
                    context,
                    Self::NAME,
                    obj_repr.to_str()?
                ));
                Ok(Some(err))
            }
        }
    }
}

// FUTURE BlocksBuilder => BuilderFromBlockScope?
/// Typeclass representing a "builder" which takes a Blocks and produces a new DocElement.
/// Doesn't typecheck the output of the build method, that's done in [`crate::interpreter::state_machines::code`]
///
/// Requires a method
/// ```python
/// def build_from_blocks(self, blocks: Blocks) -> Block | Inline | Header | None: ...
/// ```
#[derive(Debug, Clone)]
pub struct BlocksBuilder {}
impl BlocksBuilder {
    fn marker_func_name(py: Python<'_>) -> &Bound<'_, PyString> {
        intern!(py, "build_from_blocks")
    }
    pub fn call_build_from_blocks<'py>(
        py: Python<'py>,
        builder: PyTcRef<Self>,
        blocks: Py<Blocks>,
    ) -> PyResult<Bound<'py, PyAny>> {
        builder
            .bind(py)
            .getattr(Self::marker_func_name(py))?
            .call1((blocks.bind(py),))
    }
}
impl PyTypeclass for BlocksBuilder {
    const NAME: &'static str = "BlocksBuilder";

    fn fits_typeclass(obj: &Bound<'_, PyAny>) -> PyResult<bool> {
        obj.hasattr(Self::marker_func_name(obj.py()))
    }
    fn get_typeclass_err(obj: &Bound<'_, PyAny>, context: &str) -> PyResult<Option<PyErr>> {
        if Self::fits_typeclass(obj)? {
            Ok(None)
        } else {
            let obj_repr = obj.repr()?;
            let err = PyTypeError::new_err(format!(
                "Expected {} to be an instance of {}, but it didn't have a build_from_blocks() method. Got {}",
                context,
                Self::NAME,
                obj_repr.to_str()?
            ));
            Ok(Some(err))
        }
    }
}

/// Typeclass representing a "builder" which takes an InlineScope and produces a new DocElement.
////// Doesn't typecheck the output of the build method, that's done in [`crate::interpreter::state_machines::code`]

/// Requires a method
/// ```python
/// def build_from_inlines(self, inlines: InlineScope) -> Block | Inline | Header | None: ...
/// ```
#[derive(Debug, Clone)]
pub struct InlineScopeBuilder {}
impl InlineScopeBuilder {
    fn marker_func_name(py: Python<'_>) -> &Bound<'_, PyString> {
        intern!(py, "build_from_inlines")
    }
    pub fn call_build_from_inlines<'py>(
        py: Python<'py>,
        builder: PyTcRef<Self>,
        inlines: Py<InlineScope>,
    ) -> PyResult<Bound<'py, PyAny>> {
        builder
            .bind(py)
            .getattr(Self::marker_func_name(py))?
            .call1((inlines.bind(py),))
    }
}
impl PyTypeclass for InlineScopeBuilder {
    const NAME: &'static str = "InlineScopeBuilder";

    fn fits_typeclass(obj: &Bound<'_, PyAny>) -> PyResult<bool> {
        obj.hasattr(Self::marker_func_name(obj.py()))
    }
    fn get_typeclass_err(obj: &Bound<'_, PyAny>, context: &str) -> PyResult<Option<PyErr>> {
        if Self::fits_typeclass(obj)? {
            Ok(None)
        } else {
            let obj_repr = obj.repr()?;
            let err = PyTypeError::new_err(format!(
                "Expected {} to be an instance of {}, but it didn't have a build_from_inlines() method. Got {}",
                context,
                Self::NAME,
                obj_repr.to_str()?
            ));
            Ok(Some(err))
        }
    }
}

/// Typeclass representing a "builder" which takes a Raw scope and produces a new DocElement.
////// Doesn't typecheck the output of the build method, that's done in [`crate::interpreter::state_machines::code`]

/// Requires a method
/// ```python
/// def build_from_raw(self, raw: Raw) -> Block | Inline | Header | None: ...
/// ```
#[derive(Debug, Clone)]
pub struct RawScopeBuilder {}
impl RawScopeBuilder {
    fn marker_func_name(py: Python<'_>) -> &Bound<'_, PyString> {
        intern!(py, "build_from_raw")
    }
    /// Calls builder.build_from_raw(raw)
    pub fn call_build_from_raw<'py>(
        py: Python<'py>,
        builder: PyTcRef<Self>,
        raw: Py<Raw>,
    ) -> PyResult<Bound<'py, PyAny>> {
        builder
            .bind(py)
            .getattr(Self::marker_func_name(py))?
            .call1((raw,))
    }
}
impl PyTypeclass for RawScopeBuilder {
    const NAME: &'static str = "RawScopeBuilder";

    fn fits_typeclass(obj: &Bound<'_, PyAny>) -> PyResult<bool> {
        obj.hasattr(Self::marker_func_name(obj.py()))
    }
    fn get_typeclass_err(obj: &Bound<'_, PyAny>, context: &str) -> PyResult<Option<PyErr>> {
        if Self::fits_typeclass(obj)? {
            Ok(None)
        } else {
            let obj_repr = obj.repr()?;
            let err = PyTypeError::new_err(format!(
                "Expected {} to be an instance of {}, but it didn't have a build_from_raw() method. Got {}",
                context,
                Self::NAME,
                obj_repr.to_str()?
            ));
            Ok(Some(err))
        }
    }
}

/// Represents plain inline text that has not yet been "escaped" for rendering.
///
/// Typically created by Rust while parsing input files.
#[pyclass]
#[derive(Debug, Clone)]
pub struct Text(pub Py<PyString>);
impl Text {
    pub fn new_rs(py: Python, s: &str) -> Self {
        Self::new(&PyString::new_bound(py, s))
    }
}
#[pymethods]
impl Text {
    #[new]
    pub fn new(data: &Bound<'_, PyString>) -> Self {
        Self(data.as_unbound().clone())
    }
    #[getter]
    pub fn text(&self) -> PyResult<Py<PyString>> {
        Ok(self.0.clone())
    }
    #[getter]
    pub fn is_inline(&self) -> bool {
        true
    }
    pub fn __eq__(&self, py: Python, other: &Self) -> PyResult<bool> {
        self.0
            .getattr(py, intern!(py, "__eq__"))?
            .call1(py, (other.0.bind(py),))?
            .is_truthy(py)
    }
    pub fn __repr__(&self, py: Python) -> PyResult<String> {
        Ok(format!("Text({})", self.0.bind(py).repr()?.to_str()?))
    }
    #[classattr]
    const __hash__: Option<Py<PyAny>> = None;
}

/// Represents raw data that should not be escaped for rendering.
///
/// Typically created by Rust while parsing input files.
#[pyclass]
#[derive(Debug, Clone)]
pub struct Raw(pub Py<PyString>);
impl Raw {
    pub fn new_rs(py: Python, s: &str) -> Self {
        Self::new(PyString::new_bound(py, s))
    }
}
#[pymethods]
impl Raw {
    #[new]
    pub fn new(data: Bound<'_, PyString>) -> Self {
        Self(data.as_unbound().clone())
    }
    #[getter]
    pub fn data(&self) -> PyResult<Py<PyString>> {
        Ok(self.0.clone())
    }
    #[getter]
    pub fn is_inline(&self) -> bool {
        true
    }
    pub fn __eq__(&self, py: Python, other: &Self) -> PyResult<bool> {
        self.0
            .getattr(py, intern!(py, "__eq__"))?
            .call1(py, (other.0.bind(py),))?
            .is_truthy(py)
    }
    pub fn __repr__(&self, py: Python) -> PyResult<String> {
        Ok(format!("Raw({})", self.0.bind(py).repr()?.to_str()?))
    }
    #[classattr]
    const __hash__: Option<Py<PyAny>> = None;
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
    #[pyo3(signature = (seq=None))]
    pub fn new(py: Python, seq: Option<&Bound<'_, PySequence>>) -> PyResult<Self> {
        match seq {
            Some(seq) => Ok(Self(PyTypeclassList::wrap_seq(&seq)?)),
            None => Ok(Self(PyTypeclassList::new(py))),
        }
    }

    pub fn __len__(&self, py: Python) -> usize {
        self.0.list(py).len()
    }
    pub fn __iter__<'py>(&'py self, py: Python<'py>) -> PyResult<Bound<'py, PyIterator>> {
        self.0.list(py).as_sequence().iter()
    }

    pub fn append_inline(&mut self, obj: &Bound<'_, PyAny>) -> PyResult<()> {
        self.0.append_checked(obj)
    }
    pub fn insert_inline(&self, index: usize, obj: &Bound<'_, PyAny>) -> PyResult<()> {
        self.0.insert_checked(index, obj)
    }

    pub fn __eq__(&self, py: Python, other: &Self) -> PyResult<bool> {
        self.0.__eq__(py, &other.0)
    }
    pub fn __str__(&self, py: Python) -> PyResult<String> {
        Ok(format!("Sentence(<{} inlines>)", self.0.list(py).len()))
    }
    pub fn __repr__(&self, py: Python) -> PyResult<String> {
        Ok(format!(r#"Sentence({})"#, self.0.__repr__(py)?))
    }
    #[classattr]
    const __hash__: Option<Py<PyAny>> = None;
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
    #[pyo3(signature = (seq=None))]
    pub fn new(py: Python, seq: Option<&Bound<'_, PySequence>>) -> PyResult<Self> {
        match seq {
            Some(seq) => Ok(Self(PyInstanceList::wrap_seq(&seq)?)),
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
    pub fn __iter__<'py>(&'py self, py: Python<'py>) -> PyResult<Bound<'py, PyIterator>> {
        self.0.list(py).as_sequence().iter()
    }

    pub fn append_sentence(&mut self, py: Python, sentence: Py<Sentence>) -> PyResult<()> {
        self.0.append_checked(sentence.bind(py))
    }
    pub fn insert_sentence(
        &self,
        py: Python,
        index: usize,
        sentence: Py<Sentence>,
    ) -> PyResult<()> {
        self.0.insert_checked(index, sentence.bind(py))
    }

    pub fn __eq__(&self, py: Python, other: &Self) -> PyResult<bool> {
        self.0.__eq__(py, &other.0)
    }
    pub fn __str__(&self, py: Python) -> PyResult<String> {
        Ok(format!("Paragraph(<{} sentences>)", self.0.list(py).len()))
    }
    pub fn __repr__(&self, py: Python) -> PyResult<String> {
        Ok(format!(r#"Paragraph({})"#, self.0.__repr__(py)?))
    }
    #[classattr]
    const __hash__: Option<Py<PyAny>> = None;
}

/// A group of [Block]s inside non-code-preceded squiggly braces
///
/// Typically created by Rust while parsing input files.
#[pyclass(sequence)]
#[derive(Debug, Clone)]
pub struct Blocks(pub PyTypeclassList<Block>);
impl Blocks {
    pub fn new_empty(py: Python) -> Self {
        Self(PyTypeclassList::new(py))
    }
}
#[pymethods]
impl Blocks {
    #[new]
    #[pyo3(signature = (seq=None))]
    pub fn new(py: Python, seq: Option<&Bound<'_, PySequence>>) -> PyResult<Self> {
        match seq {
            Some(seq) => Ok(Self(PyTypeclassList::wrap_seq(&seq)?)),
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
    pub fn __iter__<'py>(&'py self, py: Python<'py>) -> PyResult<Bound<'py, PyIterator>> {
        self.0.list(py).as_sequence().iter()
    }

    pub fn append_block(&mut self, obj: &Bound<'_, PyAny>) -> PyResult<()> {
        self.0.append_checked(obj)
    }
    pub fn insert_block(&self, index: usize, obj: &Bound<'_, PyAny>) -> PyResult<()> {
        self.0.insert_checked(index, obj)
    }

    pub fn __eq__(&self, py: Python, other: &Self) -> PyResult<bool> {
        self.0.__eq__(py, &other.0)
    }
    pub fn __str__(&self, py: Python) -> PyResult<String> {
        Ok(format!("Blocks(<{} blocks>)", self.0.list(py).len()))
    }
    pub fn __repr__(&self, py: Python) -> PyResult<String> {
        Ok(format!(r#"Blocks({})"#, self.0.__repr__(py)?))
    }
    #[classattr]
    const __hash__: Option<Py<PyAny>> = None;
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
    #[pyo3(signature = (seq=None))]
    pub fn new(py: Python, seq: Option<&Bound<'_, PySequence>>) -> PyResult<Self> {
        match seq {
            Some(seq) => Ok(Self(PyTypeclassList::wrap_seq(&seq)?)),
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
    pub fn __iter__<'py>(&'py self, py: Python<'py>) -> PyResult<Bound<'py, PyIterator>> {
        self.0.list(py).as_sequence().iter()
    }

    pub fn append_inline(&mut self, obj: &Bound<'_, PyAny>) -> PyResult<()> {
        self.0.append_checked(obj)
    }
    pub fn insert_inline(&self, index: usize, obj: &Bound<'_, PyAny>) -> PyResult<()> {
        self.0.insert_checked(index, obj)
    }

    pub fn __eq__(&self, py: Python, other: &Self) -> PyResult<bool> {
        self.0.__eq__(py, &other.0)
    }
    pub fn __str__(&self, py: Python) -> PyResult<String> {
        Ok(format!("InlineScope(<{} inlines>)", self.0.list(py).len()))
    }
    pub fn __repr__(&self, py: Python) -> PyResult<String> {
        Ok(format!(r#"InlineScope({})"#, self.0.__repr__(py)?))
    }
    #[classattr]
    const __hash__: Option<Py<PyAny>> = None;
}

/// A source file for turnip_text parsing.
/// Python must create an instance of this to initiate parsing.
/// If block-level code emits a [TurnipTextSource] object the parser will parse the new source code before returning to the top-level file that emitted it.
#[pyclass]
#[derive(Clone)]
pub struct TurnipTextSource {
    pub name: String,
    pub contents: String,
}
impl TurnipTextSource {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn contents(&self) -> &str {
        &self.contents
    }
}
impl std::fmt::Debug for TurnipTextSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TurnipTextSource")
            .field("name", &self.name)
            .field("contents", &"<...>")
            .finish()
    }
}
#[pymethods]
impl TurnipTextSource {
    #[new]
    pub fn new(name: String, contents: String) -> TurnipTextSource {
        TurnipTextSource { name, contents }
    }

    /// Take a file object, call read(), expect the output to be a string, and pull the data out as UTF-8
    /// for turnip_text to process.
    ///
    /// Previously this method took a filepath directly and read it using Rust, but that wouldn't handle non-UTF-8-encoded files correctly.
    #[staticmethod]
    pub fn from_file(
        py: Python,
        name: String,
        file: &Bound<'_, PyAny>,
    ) -> PyResult<TurnipTextSource> {
        let file_contents = file.getattr(intern!(py, "read"))?.call0()?;
        let file_contents_str = file_contents.downcast::<PyString>()?;

        Ok(TurnipTextSource {
            name,
            contents: file_contents_str.to_str()?.to_owned(),
        })
    }

    /// Create a TurnipTextSource from a UTF-8 string, automatically naming the file "<string>".
    #[staticmethod]
    pub fn from_string(contents: String) -> TurnipTextSource {
        TurnipTextSource {
            name: "<string>".into(),
            contents,
        }
    }
}

/// A document with frontmatter `content` and the roots of a tree of [DocSegment]s.
///
/// Maintains the invariant from [DocSegmentList]:
/// - foreach adjacent pair `seg_a, seg_b` in `segments`, `seg_a.header.weight >= seg_b.header.weight`
///   i.e. `not(seg_a.header.weight < seg_b.header.weight)`
///   i.e. there are no pairs such that `seg_a` *should* contain `seg_b`
#[pyclass]
#[derive(Debug, Clone)]
pub struct Document {
    pub contents: Py<Blocks>,
    pub segments: DocSegmentList,
}
impl Document {
    pub fn empty(py: Python) -> PyResult<Self> {
        Ok(Self {
            contents: Py::new(py, Blocks::new_empty(py))?,
            segments: DocSegmentList::empty(py),
        })
    }
}
#[pymethods]
impl Document {
    #[new]
    pub fn new(contents: Py<Blocks>, segments: &Bound<'_, PySequence>) -> PyResult<Self> {
        Ok(Self {
            contents,
            segments: DocSegmentList::new(segments)?,
        })
    }
    #[getter]
    pub fn contents<'py>(&'py self, py: Python<'py>) -> &'py Bound<'py, Blocks> {
        self.contents.bind(py)
    }
    #[getter]
    pub fn segments<'py>(&'py self, py: Python<'py>) -> PyResult<Bound<'py, PyIterator>> {
        self.segments.list(py).as_sequence().iter()
    }
    pub fn append_header<'py>(
        &'py self,
        py: Python<'py>,
        new_header: &'py Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, DocSegment>> {
        // First, make sure the header is a header - this means we provide a good error message to user Python
        let new_header = PyTcRef::of_friendly(new_header, "input to .append_header()")?;
        self.segments.append_header(py, new_header)
    }
    pub fn insert_header<'py>(
        &'py self,
        py: Python<'py>,
        index: usize,
        new_header: &'py Bound<'_, PyAny>,
    ) -> PyResult<Bound<'_, DocSegment>> {
        // First, make sure the header is a header - this means we provide a good error message to user Python
        let new_header = PyTcRef::of_friendly(new_header, "input to .insert_header()")?;
        self.segments.insert_header(py, index, new_header)
    }
    pub fn __eq__(&self, py: Python, other: &Self) -> PyResult<bool> {
        Ok(self.contents.bind(py).eq(other.contents.bind(py))?
            && self.segments.__eq__(py, &other.segments)?)
    }
    pub fn __str__(&self, py: Python) -> PyResult<String> {
        Ok(format!(
            r#"Document(contents={}, segments={})"#,
            self.contents.borrow(py).__str__(py)?,
            self.segments.__repr__(py)?
        ))
    }
    pub fn __repr__(&self, py: Python) -> PyResult<String> {
        Ok(format!(
            r#"Document(contents={}, segments={})"#,
            self.contents.borrow(py).__repr__(py)?,
            self.segments.__repr__(py)?
        ))
    }
    #[classattr]
    const __hash__: Option<Py<PyAny>> = None;
}

/// A segment of a [Document] with a [Header], frontmatter `contents`, and a list of `subsegments` all with greater weight than the weight of its [Header].
///
/// Maintains two invariants:
/// - foreach entry `subseg` in `subsegments`, `self.header.weight < subseg.header.weight`
/// - foreach adjacent pair `subseg_a, subseg_b` in `subsegments`, `subseg_a.header.weight >= subseg_b.header.weight`
///   i.e. `not(subseg_a.header.weight < subseg_b.header.weight)`
///   i.e. there are no pairs such that `subseg_a` *should* contain `subseg_b`
/// Uses [DocSegmentList] to maintain the invariant that
#[pyclass]
#[derive(Debug, Clone)]
pub struct DocSegment {
    pub header: PyTcRef<Header>,
    pub contents: Py<Blocks>,
    pub subsegments: DocSegmentList,
}
#[pymethods]
impl DocSegment {
    #[new]
    pub fn new(
        header: &Bound<'_, PyAny>,
        contents: Py<Blocks>,
        subsegments: &Bound<'_, PySequence>,
    ) -> PyResult<Self> {
        // First, make sure the header is a header - this means we provide a good error message to user Python
        let py = header.py();
        let header = PyTcRef::of_friendly(header, "input to DocSegment __init__")?;

        let subsegments = DocSegmentList::new(subsegments)?;
        let weight = Header::get_weight(py, header.bind(py))?;
        for subsegment in subsegments.__iter__(py)? {
            let subweight = {
                let subsegment = subsegment?.downcast::<DocSegment>()?.borrow();
                let subheader = subsegment.header.bind(py);
                Header::get_weight(py, subheader)?
            };
            if subweight <= weight {
                return Err(PyValueError::new_err(format!(
                    "Trying to create a DocSegment with weight {weight} but one \
                    of the provided subsegments has weight {subweight} which is \
                    smaller. Only larger subweights are allowed."
                )));
            }
        }
        Ok(Self {
            header,
            contents,
            subsegments,
        })
    }
    #[getter]
    pub fn header<'py>(&'py self, py: Python<'py>) -> &'py Bound<'py, PyAny> {
        self.header.bind(py)
    }
    #[getter]
    pub fn contents<'py>(&'py self, py: Python<'py>) -> &'py Bound<'py, Blocks> {
        self.contents.bind(py)
    }
    #[getter]
    pub fn subsegments<'py>(&'py self, py: Python<'py>) -> PyResult<Bound<'py, PyIterator>> {
        self.subsegments.list(py).as_sequence().iter()
    }
    pub fn append_header<'py>(
        &'py self,
        py: Python<'py>,
        new_header: &'py Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, DocSegment>> {
        // First, make sure the header is a header - this means we provide a good error message to user Python
        let new_header = PyTcRef::of_friendly(new_header, "input to .append_header()")?;

        let weight = Header::get_weight(py, self.header.bind(py))?;
        let subweight = Header::get_weight(py, new_header.bind(py))?;
        if subweight <= weight {
            return Err(PyValueError::new_err(format!(
                "Trying to add to a DocSegment with weight {weight} but the provided \
                         subheader has weight {subweight} which is lower or equal. Only larger \
                         subweights are allowed."
            )));
        };
        self.subsegments.append_header(py, new_header)
    }
    pub fn insert_header<'py>(
        &'py self,
        py: Python<'py>,
        index: usize,
        new_header: &'py Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, DocSegment>> {
        // First, make sure the header is a header - this means we provide a good error message to user Python
        let new_header = PyTcRef::of_friendly(new_header, "input to .insert_header()")?;
        let weight = Header::get_weight(py, self.header.bind(py))?;
        let subweight = Header::get_weight(py, new_header.bind(py))?;
        if subweight <= weight {
            return Err(PyValueError::new_err(format!(
                "Trying to add to a DocSegment with weight {weight} but the provided \
                         subheader has weight {subweight} which is lower or equal. Only larger \
                         subweights are allowed."
            )));
        };
        self.subsegments.insert_header(py, index, new_header)
    }
    pub fn __eq__(&self, py: Python, other: &Self) -> PyResult<bool> {
        Ok(self.header.bind(py).eq(other.header.bind(py))?
            && self.contents.bind(py).eq(other.contents.bind(py))?
            && self.subsegments.__eq__(py, &other.subsegments)?)
    }
    pub fn __str__(&self, py: Python) -> PyResult<String> {
        Ok(format!(
            r#"DocSegment(header={}, contents={}, subsegments={})"#,
            self.header.bind(py).str()?.to_str()?,
            self.contents.borrow(py).__str__(py)?,
            self.subsegments.__repr__(py)?
        ))
    }
    pub fn __repr__(&self, py: Python) -> PyResult<String> {
        Ok(format!(
            r#"DocSegment(header={}, contents={}, subsegments={})"#,
            self.header.bind(py).str()?.to_str()?,
            self.contents.borrow(py).__repr__(py)?,
            self.subsegments.__repr__(py)?
        ))
    }
    #[classattr]
    const __hash__: Option<Py<PyAny>> = None;
}

/// INTERNAL ONLY, not released to Python.
/// Implements common behaviour for Document and DocSegment.
///
/// A list of DocSegments that maintains the invariant that for any pair of adjacent segments in the list (A, B),
/// A.header.weight >= B.header.weight i.e. there is no scenario where two segments can be adjacent when one should
/// be merged into the other.
///
/// Public interface allows insertion of Headers, returning a new DocSegment which may already have elements inside it to maintain the invariant.
///
/// Maintains one invariant:
/// - foreach adjacent pair `subseg_a, subseg_b`, `subseg_a.header.weight >= subseg_b.header.weight`
///   i.e. `not(subseg_a.header.weight < subseg_b.header.weight)`
///   i.e. there are no pairs such that `subseg_a` *should* contain `subseg_b`
///
/// This invariant is always maintained, and holds under removal of arbitrary elements
/// (e.g. for [A, B, C], `A >= B >= C` => `A >= C` => [A, C] is also valid)
#[pyclass]
#[derive(Debug, Clone)]
pub struct DocSegmentList(Py<PyList>);
impl DocSegmentList {
    pub fn empty(py: Python) -> Self {
        Self(PyList::empty_bound(py).into())
    }

    /// If the segment currently at the end of the list has a lower weight than the new segment, merge the new segment into the end of it.
    /// Otherwise insert the new segment directly to the end.
    ///
    /// i.e. for the empty list, new list = [X]
    /// otherwise the list is (stuff) + [A], if A.weight >= X.weight then get (stuff) + [A, X] else get (stuff) + [A.append(X)]
    fn append_full_subsegment(
        &self,
        new_docsegment: &Bound<'_, DocSegment>,
        new_weight: i64,
    ) -> PyResult<()> {
        let py = new_docsegment.py();
        let self_list = self.0.bind(py);
        if self_list.is_empty() {
            self_list.append(new_docsegment)
        } else {
            // Safe to call this - there are no elements after (len - 1) so no work is required to uphold the invariant for that side.
            self.merge_or_insert_after_index(new_docsegment, new_weight, self_list.len() - 1)
        }
    }

    /// If the segment currently at `index` has a lower weight than the new segment, merge the new segment into the end of it.
    /// Otherwise insert the new segment *after* `index`.
    ///
    /// Maintains the invariant for (self[index], new_docsegment), other code is required to maintain the invariant for (new_docsegment, self[index + 1], self[index + 2]...)
    fn merge_or_insert_after_index(
        &self,
        new_docsegment: &Bound<'_, DocSegment>,
        new_weight: i64,
        index: usize,
    ) -> PyResult<()> {
        let py = new_docsegment.py();
        let self_list = self.0.bind(py);
        let elem = self_list.get_item(index)?;
        let elem = elem.downcast::<DocSegment>()?;
        let elem_weight = Header::get_weight(py, &elem.borrow().header.bind(py))?;
        // We are trying to insert `new_docsegment` after `elem`.
        // We need to uphold the invariant `elem.header.weight >= new_docsegment.header.weight`.
        // If that isn't true, we need to insert `new_docsegment` inside `elem` instead.
        if elem_weight < new_weight {
            elem.borrow()
                .subsegments
                .append_full_subsegment(new_docsegment, new_weight)
        } else {
            // To insert *after* elem[index], insert *at* [index + 1]
            self_list.insert(index + 1, new_docsegment)
        }
    }

    /// Removes and returns the run of DocSegments at + after `index` which have `weight` > `new_weight`.
    /// After this function is called, `index` may be one past the end of the list
    /// Pseudocode;
    /// ```python
    /// segments = []
    /// while self[index].weight > new_weight:
    ///     segments.append_segment(self.delete_at(index))
    /// ```
    ///
    /// Maintains the invariant for (new_docsegment)
    fn extract_heavier_items<'py>(
        &'py self,
        py: Python<'py>,
        new_weight: i64,
        index: usize,
    ) -> PyResult<Bound<PyList>> {
        let new_list = PyList::empty_bound(py);
        let self_list = self.0.bind(py);
        while index < self_list.len() {
            let item_at_index = self_list.get_item(index)?;
            let segment_at_index = item_at_index.downcast::<DocSegment>()?;
            let header = &segment_at_index.borrow().header;
            let weight = Header::get_weight(py, header.bind(py))?;
            if weight <= new_weight {
                break;
            } else {
                self_list.del_item(index)?;
                new_list.append(segment_at_index)?;
            }
        }
        Ok(new_list)
    }

    pub fn list<'py>(&self, py: Python<'py>) -> &Bound<'py, PyList> {
        self.0.bind(py)
    }

    /// Create a new DocSegmentList. Raises ValueError if the invariant is not upheld.
    pub fn new(seq: &Bound<'_, PySequence>) -> PyResult<Self> {
        let py = seq.py();
        let list = PyList::empty_bound(py);

        let mut last_weight = None;
        for obj in seq.iter()? {
            let obj = obj?;
            let obj = obj.downcast::<DocSegment>()?;
            let obj_weight = Header::get_weight(py, obj.borrow().header.bind(py))?;

            match last_weight {
                Some(last_weight) => {
                    if last_weight < obj_weight {
                        return Err(PyValueError::new_err(format!("Sequence given to DocSegmentList had an element of weight {last_weight} followed by an element of weight {obj_weight}.\nThe invariant of this list is that for each pair of adjacent elements (A, B) A has a greater weight than B.")));
                    }
                }
                None => {}
            }
            last_weight = Some(obj_weight);

            list.append(obj)?
        }
        Ok(Self(list.into()))
    }

    /// Take a header, find its weight, create a DocSegment from it, and push that to the end of this list
    /// or into the subsegments of the last element to maintain the invariant.
    ///
    /// For example, if the list currently has two elements `[A, B]` and you call `append_header(X)`,
    /// `[A, B, X]` is only allowed if B.weight >= X.weight, otherwise X must be appended into B's subsegments.
    pub fn append_header<'py>(
        &self,
        py: Python<'py>,
        header: PyTcRef<Header>,
    ) -> PyResult<Bound<'py, DocSegment>> {
        let new_weight = Header::get_weight(py, header.bind(py))?;

        let new_docsegment = Py::new(
            py,
            DocSegment {
                header,
                contents: Py::new(py, Blocks::new_empty(py))?,
                subsegments: Self(PyList::empty_bound(py).into()),
            },
        )?
        .into_bound(py);

        // Maintains the invariant between the current final list element and the new element
        self.append_full_subsegment(&new_docsegment, new_weight)?;

        Ok(new_docsegment)
    }
    /// Take a header, create a DocSegment with that header and insert it into the DocSegment tree
    /// and based on its weight insert it before the element indicated by `index`.
    ///
    /// The new docsegment may not be a direct element of `self`, but may be in one of the children of `self` instead, if it comes after an element with lower weight.
    /// For example, if the list has two elements `[A, B]` and `X` is inserted between them, there are two possibilities:
    /// - `[A, X, B]` is allowed if `A.weight >= X.weight`.
    /// - `[A.append_header(X), B]` must be done if `A.weight < X.weight` to maintain the invariant.
    ///
    /// The new docsegment may be pre-created with children if its weight is smaller than subsequent elements.
    /// For example if the list has three elements `[A, B, C]` and `X` is inserted after `A`, there are four possiblities:
    /// - `[A.append(X), B, C]` is allowed if `A.weight < X.weight`
    ///     - e.g. A = 100, X = 101
    /// - `[A, X, B, C]` is allowed if `A.weight >= X.weight` and `X.weight >= B.weight`
    ///     - e.g. A = 100, X = 10, B = 5
    /// - `[A, X.append(B), C]` is allowed if `A.weight >= X.weight`, `X.weight < B.weight`, and `X.weight >= C.weight`
    ///     - e.g. A = 100, X = 10, B = 75, C = 5
    /// - `[A, X.append(B, C)]` is allowed if `A.weight >= X.weight`, `X.weight < B.weight`, and `X.weight < C.weight`
    ///     - e.g. A = 100, X = 10, B = 75, C = 50
    pub fn insert_header<'py>(
        &self,
        py: Python<'py>,
        index: usize,
        header: PyTcRef<Header>,
    ) -> PyResult<Bound<'py, DocSegment>> {
        let new_weight = Header::get_weight(py, header.bind(py))?;

        let new_docsegment = Py::new(
            py,
            DocSegment {
                header,
                contents: Py::new(py, Blocks::new_empty(py))?,
                // Enforces the invariant for elements after the insertion point
                subsegments: Self(self.extract_heavier_items(py, new_weight, index)?.into()),
            },
        )?
        .into_bound(py);

        // Need to maintain the invariant for elements before the insertion point
        if index == 0 {
            // There are no elements before the insertion point
            self.0.bind(py).insert(0, &new_docsegment)?
        } else {
            // There are elements, and this function enforces the invariant for them
            self.merge_or_insert_after_index(&new_docsegment, new_weight, index - 1)?
        }

        Ok(new_docsegment)
    }

    pub fn __len__(&self, py: Python) -> usize {
        self.0.bind(py).len()
    }
    pub fn __iter__<'py>(&'py self, py: Python<'py>) -> PyResult<Bound<'py, PyIterator>> {
        self.0.bind(py).as_sequence().iter()
    }

    pub fn __eq__(&self, py: Python, other: &Self) -> PyResult<bool> {
        self.0.bind(py).eq(other.0.bind(py))
    }
    pub fn __str__(&self, py: Python) -> PyResult<String> {
        Ok(format!(
            "DocSegmentList(<{} segments>)",
            self.0.bind(py).len()
        ))
    }
    pub fn __repr__(&self, py: Python) -> PyResult<String> {
        Ok(format!(
            r#"DocSegmentList({})"#,
            self.0.bind(py).str()?.to_str()?
        ))
    }
}
