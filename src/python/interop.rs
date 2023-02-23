use std::path::Path;

use pyo3::{
    exceptions::PyRuntimeError,
    prelude::*,
    types::{PyDict, PyIterator, PyString},
};

use super::typeclass::{PyInstanceList, PyTcRef, PyTypeclass, PyTypeclassList};

#[pymodule]
pub fn turnip_text(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse_file, m)?)?;

    // Primitives
    m.add_class::<UnescapedText>()?;
    m.add_class::<RawText>()?;
    m.add_class::<Sentence>()?;
    m.add_class::<Paragraph>()?;

    // Scopes
    m.add_class::<BlockScope>()?;
    m.add_class::<InlineScope>()?;

    Ok(())
}

/// Given a file path, calls [crate::cli::parse_file] (includes parsing, checking for syntax errors, evaluating python)
#[pyfunction]
fn parse_file(py: Python<'_>, path: &str, locals: Option<&PyDict>) -> PyResult<Py<BlockScope>> {
    // crate::cli::parse_file already surfaces the error to the user - we can just return a generic error
    crate::cli::parse_file(
        py,
        locals.unwrap_or_else(|| PyDict::new(py)),
        Path::new(path),
    )
    .map_err(|_| {
        eprintln!("Whoops! creating error");
        let err = PyRuntimeError::new_err("parse failed");
        dbg!(&err);
        err // TODO returning a PyErr causes a segfault lol
    })
}

/// Typeclass for block elements within the document tree e.g. paragraphs, block scopes.
#[derive(Debug, Clone)]
pub struct BlockNode {}
impl PyTypeclass for BlockNode {
    const NAME: &'static str = "BlockNode";

    fn fits_typeclass(obj: &PyAny) -> PyResult<bool> {
        let x = obj.is_instance_of::<BlockScope>()? || obj.is_instance_of::<Paragraph>()?;
        Ok(x)
    }
}

/// Typeclass for objects representing content that stays within a single line/sentence.
#[derive(Debug, Clone)]
pub struct InlineNode {}
impl PyTypeclass for InlineNode {
    const NAME: &'static str = "InlineNode";

    fn fits_typeclass(obj: &PyAny) -> PyResult<bool> {
        let x = obj.is_instance_of::<InlineScope>()?
            || obj.is_instance_of::<RawText>()?
            || obj.is_instance_of::<UnescapedText>()?;
        Ok(x)
    }
}

/// Typeclass representing the "owner" of a scope, which may modify how that scope is rendered.
#[derive(Debug, Clone)]
pub struct BlockScopeOwner {}
impl PyTypeclass for BlockScopeOwner {
    const NAME: &'static str = "BlockScopeOwner";

    fn fits_typeclass(obj: &PyAny) -> PyResult<bool> {
        // TODO define the typeclass
        // TestBlockScope is really for testing only
        Ok(obj.str()?.to_str()?.contains("TestBlockScope"))
    }
}

/// Typeclass representing the "owner" of a scope, which may modify how that scope is rendered.
#[derive(Debug, Clone)]
pub struct InlineScopeOwner {}
impl PyTypeclass for InlineScopeOwner {
    const NAME: &'static str = "InlineScopeOwner";

    fn fits_typeclass(obj: &PyAny) -> PyResult<bool> {
        // TODO better define the typeclass
        Ok(obj.is_callable() ||
            // For testing only
            obj.str()?.to_str()?.contains("TestInlineScope"))
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
}

/// A sequence of [InlineNode] that represents a single sentence.
///
/// Typically created by Rust while parsing input files.
#[pyclass(sequence)]
#[derive(Debug, Clone)]
pub struct Sentence(pub PyTypeclassList<InlineNode>);
#[pymethods]
impl Sentence {
    #[new]
    pub fn new(py: Python) -> Self {
        Self(PyTypeclassList::new(py))
    }

    pub fn __len__(&self, py: Python) -> usize {
        self.0.list(py).len()
    }
    pub fn __iter__<'py>(&'py self, py: Python<'py>) -> PyResult<&'py PyIterator> {
        PyIterator::from_object(py, self.0.list(py))
    }

    pub fn push_node(&mut self, node: &PyAny) -> PyResult<()> {
        self.0.append_checked(node)
    }
}

/// A sequence of [Sentence] that combine to make a complete paragraph.
///
/// Typically created by Rust while parsing input files.
#[pyclass(sequence)]
#[derive(Debug, Clone)]
pub struct Paragraph(pub PyInstanceList<Sentence>);
#[pymethods]
impl Paragraph {
    #[new]
    pub fn new(py: Python) -> Self {
        Self(PyInstanceList::new(py))
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

/// Represents a block of plain text that may contain newlines (TODO are newlines normalized to \n?)
///
/// Typically created by Rust while parsing input files.
#[pyclass]
#[derive(Debug, Clone)]
pub struct RawText {
    pub owner: Option<PyTcRef<InlineScopeOwner>>,
    pub contents: Py<PyString>,
}
impl RawText {
    pub fn new_rs(py: Python, owner: Option<PyTcRef<InlineScopeOwner>>, s: &str) -> Self {
        Self {
            owner,
            contents: PyString::new(py, s).into_py(py),
        }
    }
}
#[pymethods]
impl RawText {
    #[new]
    pub fn new(owner: Option<&PyAny>, contents: Py<PyString>) -> PyResult<Self> {
        let o = match owner {
            Some(o) => Some(PyTcRef::of(o)?),
            None => None,
        };
        Ok(Self { owner: o, contents })
    }
}

/// A block of [Paragraph]s and other [BlockNode]s, owned by a [ScopeOwner].
///
/// Explicitly created with squiggly braces e.g.
/// ```text
/// [emph]{
///     paragraph 1
///
///     paragraph 2
/// }```
#[pyclass]
#[derive(Debug, Clone)]
pub struct BlockScope {
    pub owner: Option<PyTcRef<BlockScopeOwner>>,
    pub children: PyTypeclassList<BlockNode>,
}
impl BlockScope {
    pub fn new_rs(py: Python, owner: Option<PyTcRef<BlockScopeOwner>>) -> Self {
        Self {
            owner,
            children: PyTypeclassList::new(py),
        }
    }
}
#[pymethods]
impl BlockScope {
    #[new]
    pub fn new(py: Python, owner: Option<&PyAny>) -> PyResult<Self> {
        let o = match owner {
            Some(o) => Some(PyTcRef::of(o)?),
            None => None,
        };
        Ok(Self::new_rs(py, o))
    }

    pub fn __len__(&self, py: Python) -> usize {
        self.children.list(py).len()
    }
    pub fn __iter__<'py>(&'py self, py: Python<'py>) -> PyResult<&'py PyIterator> {
        PyIterator::from_object(py, self.children.list(py))
    }

    pub fn push_node(&mut self, node: &PyAny) -> PyResult<()> {
        self.children.append_checked(node)
    }
}

/// A sequence of [UnescapedText]s and other [InlineNode]s, owned by a [ScopeOwner].
///
/// e.g. `[code]{this_is_formatted_as_code}`
#[pyclass]
#[derive(Debug, Clone)]
pub struct InlineScope {
    pub owner: Option<PyTcRef<InlineScopeOwner>>,
    pub children: PyTypeclassList<InlineNode>,
}
impl InlineScope {
    pub fn new_rs(py: Python, owner: Option<PyTcRef<InlineScopeOwner>>) -> Self {
        Self {
            owner,
            children: PyTypeclassList::new(py),
        }
    }
}
#[pymethods]
impl InlineScope {
    #[new]
    pub fn new(py: Python, owner: Option<&PyAny>) -> PyResult<Self> {
        let o = match owner {
            Some(o) => Some(PyTcRef::of(o)?),
            None => None,
        };
        Ok(Self::new_rs(py, o))
    }

    pub fn __len__(&self, py: Python) -> usize {
        self.children.list(py).len()
    }
    pub fn __iter__<'py>(&'py self, py: Python<'py>) -> PyResult<&'py PyIterator> {
        PyIterator::from_object(py, self.children.list(py))
    }

    pub fn push_node(&mut self, node: &PyAny) -> PyResult<()> {
        self.children.append_checked(node)
    }
}

pub enum EvalBracketResult {
    Block(PyTcRef<BlockScopeOwner>),
    Inline(PyTcRef<InlineScopeOwner>),
    Other(Py<PyString>),
}
impl EvalBracketResult {
    pub fn eval(py: Python, globals: &PyDict, code: &str) -> PyResult<EvalBracketResult> {
        // Python picks up leading whitespace as an incorrect indent
        let code = code.trim();
        let raw_res = py.eval(code, Some(globals), None)?;
        let res = if let Ok(val) = PyTcRef::of(raw_res) {
            EvalBracketResult::Inline(val)
        } else if let Ok(val) = PyTcRef::of(raw_res) {
            EvalBracketResult::Block(val)
        } else {
            EvalBracketResult::Other(raw_res.str()?.into_py(py))
        };
        Ok(res)
    }
}
