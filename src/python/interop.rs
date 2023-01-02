use pyo3::{prelude::*, types::{PyString, PyDict}, PyClass, AsPyPointer};
use pyo3::exceptions::PyTypeError;

/// If the given value is an instance of `T` as determined by [PyAny::is_instance_of],
/// downcast it and create a GIL-independent reference.
/// Otherwise return None.
/// 
/// Unlike [PyAny::downcast] this is not limited to Python native types,
/// unlike [pyo3::FromPyObject::extract] this does not clone any values.
pub fn try_downcast_ref<T: PyClass>(x: &PyAny) -> PyResult<Option<Py<T>>> {
    if x.is_instance_of::<T>()? {
        // TODO make sure this isn't incrementing the refcount too much?
        Ok(Some(unsafe { Py::from_borrowed_ptr(x.py(), x.as_ptr()) }))
    } else {
        Ok(None)
    }
}

/// If the given value is an instance of `T` as determined by [PyAny::is_instance_of],
/// downcast it and create a GIL-independent reference.
/// Otherwise raise a [PyTypeError].
/// 
/// Unlike [PyAny::downcast] this is not limited to Python native types,
/// unlike [pyo3::FromPyObject::extract] this does not clone any values.
pub fn downcast_ref<T: PyClass>(x: &PyAny) -> PyResult<Py<T>> {
    match try_downcast_ref::<T>(x)? {
        Some(x) => Ok(x),
        None => Err(PyTypeError::new_err(format!("Expected object of type {}, got {}", T::NAME, x.str()?)))
    }
}

/// [downcast_ref] for a type that is already GIL-independent.
/// Requires the GIL (i.e. [Python]) to call is_instance_of.
pub fn downcast_gil_ref<T: PyClass, TBase: PyClass>(py: Python, x: Py<TBase>) -> PyResult<Py<T>> {
    downcast_ref(x.into_ref(py))
}

#[pymodule]
pub fn turnip_text(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(experiment, m)?)?;

    // Primitives
    m.add_class::<BlockNode>()?;
    m.add_class::<InlineNode>()?;
    m.add_class::<UnescapedText>()?;
    m.add_class::<RawText>()?;
    m.add_class::<Sentence>()?;
    m.add_class::<Paragraph>()?;

    // Scopes
    m.add_class::<BlockScopeOwner>()?;
    m.add_class::<BlockScope>()?;
    m.add_class::<InlineScopeOwner>()?;
    m.add_class::<InlineScope>()?;

    Ok(())
}

#[pyfunction]
fn experiment() -> PyResult<usize> {
    eprintln!("called experiment");
    Ok(42)
}

/// Parent class for block elements within the document tree e.g. paragraphs, block scopes.
#[pyclass(subclass)]
pub struct BlockNode {}
#[pymethods]
impl BlockNode {
    #[new]
    pub fn new() -> Self {
        Self {}
    }
}

/// Parent class for objects representing content that stays within a single line/sentence.
#[pyclass(subclass)]
pub struct InlineNode {}
#[pymethods]
impl InlineNode {
    #[new]
    pub fn new() -> Self {
        Self {}
    }
}

/// Represents plain inline text that has not yet been "escaped" for rendering.
///
/// Typically created by Rust while parsing input files.
#[pyclass(extends=InlineNode, subclass)]
pub struct UnescapedText(Py<PyString>);
impl UnescapedText {
    pub fn new_rs(py: Python, s: &str) -> (Self, InlineNode) {
        Self::new(PyString::new(py, s).into_py(py))
    }
}
#[pymethods]
impl UnescapedText {
    #[new]
    pub fn new(data: Py<PyString>) -> (Self, InlineNode) {
        (Self(data), InlineNode::new())
    }
}

/// A sequence of [InlineNode] that represents a single sentence.
///
/// Typically created by Rust while parsing input files.
#[pyclass(subclass, sequence)]
pub struct Sentence(Vec<Py<InlineNode>>);
#[pymethods]
impl Sentence {
    #[new]
    pub fn new() -> Self {
        Self(vec![])
    }

    pub fn __len__(&self) -> usize {
        self.0.len()
    }

    pub fn push_node(&mut self, node: &PyAny) -> PyResult<()> {
        Ok(self.0.push(downcast_ref(node)?))
    }
}

/// A sequence of [Sentence] that combine to make a complete paragraph.
///
/// Typically created by Rust while parsing input files.
#[pyclass(extends=BlockNode, subclass, sequence)]
pub struct Paragraph(Vec<Py<Sentence>>);
#[pymethods]
impl Paragraph {
    #[new]
    pub fn new() -> (Self, BlockNode) {
        (Self(vec![]), BlockNode::new())
    }

    pub fn __len__(&self) -> usize {
        self.0.len()
    }
    
    pub fn push_sentence(&mut self, node: &PyAny) -> PyResult<()> {
        Ok(self.0.push(downcast_ref(node)?))
    }
}

/// Represents a block of plain text that may contain newlines (TODO are newlines normalized to \n?)
///
/// Typically created by Rust while parsing input files.
#[pyclass(extends=InlineNode, subclass)]
pub struct RawText(Py<PyString>);
impl RawText {
    pub fn new_rs(py: Python, s: &str) -> (Self, InlineNode) {
        Self::new(PyString::new(py, s).into_py(py))
    }
}
#[pymethods]
impl RawText {
    #[new]
    pub fn new(data: Py<PyString>) -> (Self, InlineNode) {
        (Self(data), InlineNode::new())
    }
}

/// A parent class (subclassed in Python) representing the "owner" of a scope,
/// which may modify how that scope is rendered.
#[pyclass(subclass)]
pub struct BlockScopeOwner {}
#[pymethods]
impl BlockScopeOwner {
    #[new]
    pub fn new() -> Self {
        Self {}
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
#[pyclass(extends=BlockNode, subclass)]
pub struct BlockScope {
    owner: Option<Py<BlockScopeOwner>>,
    children: Vec<Py<BlockNode>>,
}
impl BlockScope {
    pub fn new_rs(owner: Option<Py<BlockScopeOwner>>) -> (Self, BlockNode) {
        (Self { owner, children: vec![] }, BlockNode::new())
    }
}
#[pymethods]
impl BlockScope {
    #[new]
    pub fn new(owner: Option<PyRef<BlockScopeOwner>>) -> (Self, BlockNode) {
        Self::new_rs(owner.map(|o| o.into()))
    }

    pub fn __len__(&self) -> usize {
        self.children.len()
    }

    pub fn push_node(&mut self, node: &PyAny) -> PyResult<()> {
        Ok(self.children.push(downcast_ref(node)?))
    }
}
impl Default for BlockScope {
    fn default() -> Self {
        BlockScope { owner: None, children: vec![] }
    }
}

/// A parent class (subclassed in Python) representing the "owner" of a scope,
/// which may modify how that scope is rendered.
#[pyclass(subclass)]
pub struct InlineScopeOwner {}
#[pymethods]
impl InlineScopeOwner {
    #[new]
    pub fn new() -> Self {
        Self {}
    }
}

/// A sequence of [UnescapedText]s and other [InlineNode]s, owned by a [ScopeOwner].
///
/// e.g. `[code]{this_is_formatted_as_code}`
#[pyclass(extends=InlineNode, subclass)]
pub struct InlineScope {
    owner: Option<Py<InlineScopeOwner>>,
    children: Vec<Py<InlineNode>>,
}
impl InlineScope {
    pub fn new_rs(owner: Option<Py<InlineScopeOwner>>) -> (Self, InlineNode) {
        (Self { owner, children: vec![] }, InlineNode::new())
    }
}
#[pymethods]
impl InlineScope {
    #[new]
    pub fn new(owner: Option<PyRef<InlineScopeOwner>>) -> (Self, InlineNode) {
        Self::new_rs(owner.map(|o| o.into()))
    }

    pub fn __len__(&self) -> usize {
        self.children.len()
    }

    pub fn push_node(&mut self, node: &PyAny) -> PyResult<()> {
        Ok(self.children.push(downcast_ref(node)?))
    }
}

pub enum EvalBracketResult {
    Block(Py<BlockScopeOwner>),
    Inline(Py<InlineScopeOwner>),
    Other(Py<PyString>),
}
impl EvalBracketResult {
    pub fn eval(py: Python, globals: &PyDict, code: &str) -> PyResult<EvalBracketResult> {
        let raw_res = py.eval(code, Some(globals), None)?;
        let res = if let Some(val) = try_downcast_ref::<InlineScopeOwner>(raw_res)? {
            EvalBracketResult::Inline(val)
        } else if let Some(val) = try_downcast_ref::<BlockScopeOwner>(raw_res)? {
            EvalBracketResult::Block(val)
        } else {
            EvalBracketResult::Other(raw_res.str()?.into_py(py))
        };
        Ok(res)
    }
}