use pyo3::{prelude::*, types::PyString};

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
#[pyclass(subclass)]
pub struct Sentence(Vec<Py<InlineNode>>);

/// A sequence of [Sentence] that combine to make a complete paragraph.
///
/// Typically created by Rust while parsing input files.
#[pyclass(extends=BlockNode, subclass)]
pub struct Paragraph(Vec<Py<Sentence>>);

/// Represents a block of plain text that may contain newlines (TODO are newlines normalized to \n?)
///
/// Typically created by Rust while parsing input files.
#[pyclass(extends=InlineNode, subclass)]
pub struct RawText(Py<PyString>);
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
#[pymethods]
impl BlockScope {
    #[new]
    pub fn new() -> (Self, BlockNode) {
        (Self { owner: None, children: vec![] }, BlockNode::new())
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

/// A sequence of [UnescapedText]s and other [InlineNode]s, owned by a [ScopeOwner].
///
/// e.g. `[code]{this_is_formatted_as_code}`
#[pyclass(extends=InlineNode, subclass)]
pub struct InlineScope {
    owner: Option<Py<InlineScopeOwner>>,
    children: Vec<Py<InlineNode>>,
}
