use pyo3::{
    create_exception,
    exceptions::{PyTypeError, PyValueError},
    intern,
    prelude::*,
    types::{PyDict, PyFloat, PyIterator, PyList, PyLong, PyString},
};

use crate::interpreter::TurnipTextParser;

use super::typeclass::{PyInstanceList, PyTcRef, PyTypeclass, PyTypeclassList};

create_exception!(_native, TurnipTextError, pyo3::exceptions::PyException);

#[pymodule]
#[pyo3(name = "_native")]
pub fn turnip_text(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse_file, m)?)?;
    m.add_function(wrap_pyfunction!(coerce_to_inline, m)?)?;
    m.add_function(wrap_pyfunction!(coerce_to_inline_scope, m)?)?;
    m.add_function(wrap_pyfunction!(coerce_to_block, m)?)?;
    m.add_function(wrap_pyfunction!(coerce_to_block_scope, m)?)?;

    // Primitives
    m.add_class::<Text>()?;
    m.add_class::<Raw>()?;
    m.add_class::<Sentence>()?;
    m.add_class::<Paragraph>()?;
    m.add_class::<BlockScope>()?;
    m.add_class::<InlineScope>()?;
    m.add_class::<Document>()?;
    m.add_class::<DocSegment>()?;
    m.add_class::<TurnipTextSource>()?;

    m.add("TurnipTextError", py.get_type_bound::<TurnipTextError>())?;

    Ok(())
}

#[pyfunction]
fn parse_file<'py>(
    py: Python<'py>,
    file: TurnipTextSource,
    py_env: &Bound<'_, PyDict>,
) -> PyResult<Py<Document>> {
    match TurnipTextParser::oneshot_parse(py, py_env, file) {
        Ok(doc) => Ok(doc),
        Err(tterr) => Err(tterr.to_pyerr(py)),
    }
}

#[pyfunction]
pub fn coerce_to_inline<'py>(py: Python<'py>, obj: &Bound<'py, PyAny>) -> PyResult<PyObject> {
    Ok(coerce_to_inline_pytcref(py, obj)?.unbox())
}

pub fn coerce_to_inline_pytcref<'py>(
    py: Python<'py>,
    obj: &Bound<'py, PyAny>,
) -> PyResult<PyTcRef<Inline>> {
    // 1. if it's already Inline, return it
    if let Ok(inl) = PyTcRef::of(obj) {
        return Ok(inl);
    }
    // 2. if it's a List of Inline, return InlineScope(it)
    // Here we first check if it's a list, then if so try to create an InlineScope - this will verify if it's a list of Inlines.
    if let Ok(py_list) = obj.downcast::<PyList>() {
        if let Ok(inline_scope) = InlineScope::new(py, Some(&py_list)) {
            let inline_scope = Py::new(py, inline_scope)?;
            return Ok(PyTcRef::of_unchecked(inline_scope.bind(py)));
        }
    }
    // 3. if it's str, return Text(it)
    if let Ok(py_str) = obj.downcast::<PyString>() {
        let unescaped_text = Py::new(py, Text::new(py_str))?;
        return Ok(PyTcRef::of_unchecked(unescaped_text.bind(py)));
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
        InlineScope::new(py, Some(&PyList::new_bound(py, [obj])))?,
    )?);
}

#[pyfunction]
pub fn coerce_to_block<'py>(py: Python<'py>, obj: &Bound<'py, PyAny>) -> PyResult<PyObject> {
    Ok(coerce_to_block_pytcref(py, obj)?.unbox())
}

pub fn coerce_to_block_pytcref<'py>(
    py: Python<'py>,
    obj: &Bound<'py, PyAny>,
) -> PyResult<PyTcRef<Block>> {
    // 1. if it's already Block, return it
    if let Ok(block) = PyTcRef::of(obj) {
        return Ok(block);
    }
    // 2. if it's a list of Block, wrap it in a BlockScope and return it
    // Here we first check if it's a list, then if so try to create a BlockScope - this will verify if it's a list of Blocks.
    if let Ok(py_list) = obj.downcast::<PyList>() {
        if let Ok(block_scope) = BlockScope::new(py, Some(&py_list)) {
            let block_scope = Py::new(py, block_scope)?;
            return Ok(PyTcRef::of_unchecked(block_scope.bind(py)));
        }
    }
    // 3. if it's a Sentence, wrap it in a list -> Paragraph
    if let Ok(sentence) = obj.extract::<Py<Sentence>>() {
        let paragraph = Py::new(
            py,
            Paragraph::new(py, Some(&PyList::new_bound(py, [sentence])))?,
        )?;
        return Ok(PyTcRef::of_unchecked(paragraph.bind(py)));
    }
    // 4. if it can be coerced to an Inline, wrap that in list -> Sentence -> list -> Paragraph and return it
    if let Ok(inl) = coerce_to_inline(py, obj) {
        let paragraph = Py::new(
            py,
            Paragraph::new(
                py,
                Some(&PyList::new_bound(
                    py,
                    [Py::new(
                        py,
                        Sentence::new(py, Some(&PyList::new_bound(py, [inl])))?,
                    )?],
                )),
            )?,
        )?;
        return Ok(PyTcRef::of_unchecked(paragraph.bind(py)));
    }
    // 5. otherwise fail with TypeError
    Err(PyTypeError::new_err(
        "Failed to coerce object to Block: was not a Block, list of Blocks (coercible to \
         BlockScope), Paragraph, Sentence, or coercible to Inline.",
    ))
}

#[pyfunction]
pub fn coerce_to_block_scope<'py>(
    py: Python<'py>,
    obj: &Bound<'py, PyAny>,
) -> PyResult<Py<BlockScope>> {
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
        BlockScope::new(py, Some(&PyList::new_bound(py, [obj])))?,
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
/// and defines how it interacts with other segments through the weight parameter. See [DocSegment].
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

/// The possible options that can be returned by a builder.
pub enum BuilderOutcome {
    Block(PyTcRef<Block>),
    Inline(PyTcRef<Inline>),
    Header(PyTcRef<Header>),
    None,
}
impl BuilderOutcome {
    fn of_friendly(val: &Bound<'_, PyAny>, context: &str) -> PyResult<BuilderOutcome> {
        if val.is_none() {
            Ok(BuilderOutcome::None)
        } else {
            let is_block = Block::fits_typeclass(val)?;
            let is_inline = Inline::fits_typeclass(val)?;
            let is_header = Header::fits_typeclass(val)?;

            match (is_block, is_inline, is_header) {
                (true, false, false) => Ok(BuilderOutcome::Block(PyTcRef::of_unchecked(val))),
                (false, true, false) => Ok(BuilderOutcome::Inline(PyTcRef::of_unchecked(val))),
                (false, false, true) => Ok(BuilderOutcome::Header(PyTcRef::of_unchecked(val))),

                (false, false, false) => {
                    let obj_repr = val.repr()?;
                    Err(PyTypeError::new_err(format!(
                        "Expected {} to be None or an object fitting Block, Inline, or Header - got {} which fits none of them.",
                        context,
                        obj_repr.to_str()?
                    )))
                }
                _ => {
                    let obj_repr = val.repr()?;
                    Err(PyTypeError::new_err(format!(
                        "Expected {} to be None or an object fitting Block, Inline, or Header \
                         - got {} which fits (block? {}) (inline? {}) (header? {}).",
                        context,
                        obj_repr.to_str()?,
                        is_block,
                        is_inline,
                        is_header
                    )))
                }
            }
        }
    }
}

/// Typeclass representing the "builder" of a block scope, which may modify how that scope is rendered.
///
/// Requires a method
/// ```python
/// def build_from_blocks(self, blocks: BlockScope) -> Block | Inline | Header | None: ...
/// ```
#[derive(Debug, Clone)]
pub struct BlockScopeBuilder {}
impl BlockScopeBuilder {
    fn marker_func_name(py: Python<'_>) -> &Bound<'_, PyString> {
        intern!(py, "build_from_blocks")
    }
    pub fn call_build_from_blocks<'py>(
        py: Python<'py>,
        builder: PyTcRef<Self>,
        blocks: Py<BlockScope>,
    ) -> PyResult<BuilderOutcome> {
        let output = builder
            .bind(py)
            .getattr(Self::marker_func_name(py))?
            .call1((blocks.bind(py),))?;
        BuilderOutcome::of_friendly(&output, "output of .build_from_blocks()")
    }
}
impl PyTypeclass for BlockScopeBuilder {
    const NAME: &'static str = "BlockScopeBuilder";

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

/// Typeclass representing the "builder" of an inline scope, which may modify how that scope is rendered.
///
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
    ) -> PyResult<BuilderOutcome> {
        let output = builder
            .bind(py)
            .getattr(Self::marker_func_name(py))?
            .call1((inlines.bind(py),))?;
        BuilderOutcome::of_friendly(&output, "output of .build_from_inlines()")
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

/// Typeclass representing the "builder" of a raw scope, which interprets how that scope is rendered.
///
/// Requires a method
/// ```python
/// def build_from_raw(self, raw: str) -> Block | Inline | Header | None: ...
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
        raw: Py<PyString>,
    ) -> PyResult<BuilderOutcome> {
        let output = builder
            .bind(py)
            .getattr(Self::marker_func_name(py))?
            .call1((raw,))?;
        BuilderOutcome::of_friendly(&output, "output of .build_from_raw()")
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
    #[pyo3(signature = (list=None))]
    pub fn new(py: Python, list: Option<&Bound<'_, PyList>>) -> PyResult<Self> {
        match list {
            Some(list) => Ok(Self(PyTypeclassList::from(list)?)),
            None => Ok(Self(PyTypeclassList::new(py))),
        }
    }

    pub fn __len__(&self, py: Python) -> usize {
        self.0.list(py).len()
    }
    pub fn __iter__<'py>(&'py self, py: Python<'py>) -> PyResult<Bound<'py, PyIterator>> {
        self.0.list(py).as_sequence().iter()
    }

    pub fn push_inline(&mut self, node: &Bound<'_, PyAny>) -> PyResult<()> {
        self.0.append_checked(node)
    }

    pub fn __eq__(&self, py: Python, other: &Self) -> PyResult<bool> {
        self.0.__eq__(py, &other.0)
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
    #[pyo3(signature = (list=None))]
    pub fn new(py: Python, list: Option<&Bound<'_, PyList>>) -> PyResult<Self> {
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
    pub fn __iter__<'py>(&'py self, py: Python<'py>) -> PyResult<Bound<'py, PyIterator>> {
        self.0.list(py).as_sequence().iter()
    }

    pub fn push_sentence(&mut self, py: Python, sentence: Py<Sentence>) -> PyResult<()> {
        self.0.append_checked(sentence.bind(py))
    }

    pub fn __eq__(&self, py: Python, other: &Self) -> PyResult<bool> {
        self.0.__eq__(py, &other.0)
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
    pub fn new(py: Python, list: Option<&Bound<'_, PyList>>) -> PyResult<Self> {
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
    pub fn __iter__<'py>(&'py self, py: Python<'py>) -> PyResult<Bound<'py, PyIterator>> {
        self.0.list(py).as_sequence().iter()
    }

    pub fn push_block(&mut self, node: &Bound<'_, PyAny>) -> PyResult<()> {
        self.0.append_checked(node)
    }

    pub fn __eq__(&self, py: Python, other: &Self) -> PyResult<bool> {
        self.0.__eq__(py, &other.0)
    }
    pub fn __repr__(&self, py: Python) -> PyResult<String> {
        Ok(format!(r#"BlockScope({})"#, self.0.__repr__(py)?))
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
    #[pyo3(signature = (list=None))]
    pub fn new(py: Python, list: Option<&Bound<'_, PyList>>) -> PyResult<Self> {
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
    pub fn __iter__<'py>(&'py self, py: Python<'py>) -> PyResult<Bound<'py, PyIterator>> {
        self.0.list(py).as_sequence().iter()
    }

    pub fn push_inline(&mut self, node: &Bound<'_, PyAny>) -> PyResult<()> {
        self.0.append_checked(node)
    }

    pub fn __eq__(&self, py: Python, other: &Self) -> PyResult<bool> {
        self.0.__eq__(py, &other.0)
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
    /// for turnip-text to process.
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

#[pyclass]
#[derive(Debug, Clone)]
pub struct Document {
    pub contents: Py<BlockScope>,
    pub segments: PyInstanceList<DocSegment>,
}
impl Document {
    pub fn new_rs(contents: Py<BlockScope>, segments: PyInstanceList<DocSegment>) -> Self {
        Self { contents, segments }
    }
}
#[pymethods]
impl Document {
    #[new]
    pub fn new(contents: Py<BlockScope>, segments: &Bound<'_, PyList>) -> PyResult<Self> {
        Ok(Self {
            contents,
            segments: PyInstanceList::from(segments)?,
        })
    }
    #[getter]
    pub fn contents<'py>(&'py self, py: Python<'py>) -> &'py Bound<'py, BlockScope> {
        self.contents.bind(py)
    }
    #[getter]
    pub fn segments<'py>(&'py self, py: Python<'py>) -> PyResult<Bound<'py, PyIterator>> {
        self.segments.list(py).as_sequence().iter()
    }
    pub fn push_segment(&self, py: Python<'_>, segment: Py<DocSegment>) -> PyResult<()> {
        self.segments.append_checked(segment.bind(py))
    }
    pub fn __eq__(&self, py: Python, other: &Self) -> PyResult<bool> {
        Ok(self.contents.bind(py).eq(other.contents.bind(py))?
            && self.segments.__eq__(py, &other.segments)?)
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
/// Thus we allow manual scopes to be opened and closed as usual, but we don't allow DocSegments *within* them.
/// Effectively DocSegments must only exist at the "top level" - they may be enclosed by DocSegments directly, but nothing else.
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
    pub header: PyTcRef<Header>,
    pub contents: Py<BlockScope>,
    pub subsegments: PyInstanceList<DocSegment>,
}
impl DocSegment {
    pub fn new_checked(
        py: Python,
        header: PyTcRef<Header>,
        contents: Py<BlockScope>,
        subsegments: PyInstanceList<DocSegment>,
    ) -> PyResult<Self> {
        let weight = Header::get_weight(py, header.bind(py))?;
        for subsegment in subsegments.list(py).iter() {
            let subweight = {
                let subsegment: Py<DocSegment> = subsegment.extract()?;
                let subsegment = subsegment.borrow(py);
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
}
#[pymethods]
impl DocSegment {
    #[new]
    pub fn new(
        header: &Bound<'_, PyAny>,
        contents: Py<BlockScope>,
        subsegments: &Bound<'_, PyList>,
    ) -> PyResult<Self> {
        Ok(Self {
            header: PyTcRef::of_friendly(header, "input to DocSegment __init__")?,
            contents,
            subsegments: PyInstanceList::from(subsegments)?,
        })
    }
    #[getter]
    pub fn header<'py>(&'py self, py: Python<'py>) -> &'py Bound<'py, PyAny> {
        self.header.bind(py)
    }
    #[getter]
    pub fn contents<'py>(&'py self, py: Python<'py>) -> &'py Bound<'py, BlockScope> {
        self.contents.bind(py)
    }
    #[getter]
    pub fn subsegments<'py>(&'py self, py: Python<'py>) -> PyResult<Bound<'py, PyIterator>> {
        self.subsegments.list(py).as_sequence().iter()
    }
    pub fn push_subsegment(&self, py: Python<'_>, subsegment: Py<DocSegment>) -> PyResult<()> {
        let weight = Header::get_weight(py, self.header.bind(py))?;
        let subweight = Header::get_weight(py, subsegment.borrow(py).header.bind(py))?;
        if subweight <= weight {
            return Err(PyValueError::new_err(format!(
                "Trying to add to a DocSegment with weight {weight} but the provided \
                         subsegment has weight {subweight} which is smaller. Only larger \
                         subweights are allowed."
            )));
        };
        self.subsegments.append_checked(subsegment.bind(py))
    }
    pub fn __eq__(&self, py: Python, other: &Self) -> PyResult<bool> {
        Ok(self.header.bind(py).eq(other.header.bind(py))?
            && self.contents.bind(py).eq(other.contents.bind(py))?
            && self.subsegments.__eq__(py, &other.subsegments)?)
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
