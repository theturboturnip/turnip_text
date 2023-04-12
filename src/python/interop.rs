use std::{path::Path, str::FromStr};

use citeproc::{
    csl::{CslType, Variable},
    prelude::*,
    SupportedFormat,
};
use fnv::FnvHashMap;
use pyo3::{
    exceptions::PyRuntimeError,
    intern,
    prelude::*,
    types::{PyDict, PyIterator, PyList, PyString},
};

use super::typeclass::{PyInstanceList, PyTcRef, PyTypeclass, PyTypeclassList};

#[pymodule]
pub fn turnip_text(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse_file, m)?)?;
    m.add_function(wrap_pyfunction!(parse_str, m)?)?;

    // Primitives
    m.add_class::<UnescapedText>()?;
    m.add_class::<RawText>()?;
    m.add_class::<Sentence>()?;
    m.add_class::<Paragraph>()?;
    m.add_class::<BlockScope>()?;
    m.add_class::<InlineScope>()?;

    Ok(())
}

/// Given a file path, calls [crate::cli::parse_file] (includes parsing, checking for syntax errors, evaluating python)
#[pyfunction]
fn parse_file<'py>(
    py: Python<'py>,
    path: &str,
    locals: Option<&PyDict>,
) -> PyResult<Py<BlockScope>> {
    // crate::cli::parse_file already surfaces the error to the user - we can just return a generic error
    crate::cli::parse_file(
        py,
        locals.unwrap_or_else(|| PyDict::new(py)),
        Path::new(path),
    )
    .map_err(|_| PyRuntimeError::new_err("parse failed, see stdout"))
}

#[pyfunction]
fn parse_str<'py>(
    py: Python<'py>,
    data: &str,
    locals: Option<&PyDict>,
) -> PyResult<Py<BlockScope>> {
    // crate::cli::parse_str already surfaces the error to the user - we can just return a generic error
    crate::cli::parse_str(py, locals.unwrap_or_else(|| PyDict::new(py)), data)
        .map_err(|_| PyRuntimeError::new_err("parse failed, see stdout"))
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
    ) -> PyResult<PyTcRef<Block>> {
        let output = builder
            .as_ref(py)
            .getattr(Self::marker_func_name(py))?
            .call1((blocks,))?;
        PyTcRef::of(output)
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
    /// Calls builder.build_from_raw(raw)  
    pub fn call_build_from_raw<'py>(
        py: Python<'py>,
        builder: PyTcRef<Self>,
        raw: String,
    ) -> PyResult<PyTcRef<Inline>> {
        let output = builder
            .as_ref(py)
            .getattr(Self::marker_func_name(py))?
            .call1((raw,))?;
        PyTcRef::of(output)
    }
}
impl PyTypeclass for RawScopeBuilder {
    const NAME: &'static str = "RawScopeBuilder";

    fn fits_typeclass(obj: &PyAny) -> PyResult<bool> {
        obj.hasattr(Self::marker_func_name(obj.py()))
    }
}

/// Represents plain inline text that has not yet been "escaped" for rendering.
///
/// Typically created by Rust while parsing input files.
#[pyclass]
#[derive(Debug, Clone)]
pub struct UnescapedText(pub Py<PyString>);
impl UnescapedText {
    pub fn new_rs(py: Python, s: &str) -> Self {
        Self::new(PyString::new(py, s).into_py(py))
    }
}
#[pymethods]
impl UnescapedText {
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
    pub fn new(py: Python, list: Option<Py<PyList>>) -> PyResult<Self> {
        match list {
            Some(list) => Ok(Self(PyTypeclassList::from(py, list)?)),
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
    pub fn new(py: Python, list: Option<Py<PyList>>) -> PyResult<Self> {
        match list {
            Some(list) => Ok(Self(PyInstanceList::from(py, list)?)),
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
    pub fn new(py: Python, list: Option<Py<PyList>>) -> PyResult<Self> {
        match list {
            Some(list) => Ok(Self(PyTypeclassList::from(py, list)?)),
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
    pub fn new(py: Python, list: Option<Py<PyList>>) -> PyResult<Self> {
        match list {
            Some(list) => Ok(Self(PyTypeclassList::from(py, list)?)),
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

trait FromPyArg<T>: Sized {
    fn convert_py(val: T) -> PyResult<Self>;
}
impl FromPyArg<&str> for citeproc::SupportedFormat {
    fn convert_py(val: &str) -> PyResult<Self> {
        match val.to_lowercase().as_str() {
            "html" => Ok(citeproc::SupportedFormat::Html),
            "plain" => Ok(citeproc::SupportedFormat::Plain),
            "rtf" => Ok(citeproc::SupportedFormat::Rtf),
            _ => Err(PyRuntimeError::new_err(format!("Couldn't convert {val} to a valid citeproc format - expected 'html', 'plain', or 'rtf'")))
        }
    }
}

enum CslStyleXml {
    DependentStyle {
        parent_id: String,
        locale_override: Option<citeproc::csl::Lang>,
    },
    Independent,
}
#[pyclass]
struct CslCiteProcessor {
    proc: citeproc::Processor,
}
impl CslCiteProcessor {
    fn parse_meta_style(xml: &str) -> Result<CslStyleXml, citeproc::csl::StyleError> {
        let meta_style = citeproc::csl::StyleMeta::parse(xml)?;
        match meta_style.independent_parent_id() {
            None => Ok(CslStyleXml::Independent),
            Some(parent_id) => Ok(CslStyleXml::DependentStyle {
                parent_id,
                locale_override: meta_style.default_locale,
            }),
        }
    }

    fn build_proc(
        format: SupportedFormat,
        link_anchors: bool,
        base_style_xml: &str,
    ) -> Result<citeproc::Processor, citeproc::csl::StyleError> {
        // match Self::parse_meta_style(base_style_xml)? {
        //     CslStyleXml::DependentStyle { parent_id, locale_override } => todo!("Fetch parent XML and override locale"),
        //     CslStyleXml::Independent => {}
        // };

        citeproc::Processor::new(citeproc::InitOptions {
            format: format,
            format_options: citeproc::FormatOptions {
                link_anchors: link_anchors,
            },

            style: &base_style_xml,
            locale_override: None,
            fetcher: None,      // TODO locale fetcher support?
            csl_features: None, // TODO manual csl feature suppport?
            ..Default::default()
        })
    }

    fn py_obj_to_ref(python: Python, obj: &PyDict) -> Reference {
        let mut ordinary = FnvHashMap::default();
        let mut number = FnvHashMap::default();
        let mut name = FnvHashMap::default();
        let mut date = FnvHashMap::default();

        for (key, value) in obj.into_iter() {}

        Reference {
            id: todo!(),
            csl_type: CslType::from_str(todo!("String for CslType e.g, book")),
            language: None,
            ordinary: todo!(),
            number: todo!(),
            name: todo!(),
            date: todo!(),
        }
    }
}
#[pymethods]
impl CslCiteProcessor {
    #[new]
    #[pyo3()]
    pub fn new(format: &str, link_anchors: bool, base_style_xml: &str) -> PyResult<Self> {
        let format = SupportedFormat::convert_py(format)?;

        let proc = Self::build_proc(format, link_anchors, base_style_xml).map_err(|e| {
            PyRuntimeError::new_err(format!("Failed to init citeproc Processor - {}", e))
        })?;

        Ok(Self { proc })
    }

    pub fn process_citations(&mut self, py: Python) {
        self.proc.extend_references(vec![])
        // self.proc.extend_references(refs).compute()
    }
}
