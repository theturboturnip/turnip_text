use pyo3::{prelude::*, types::PyString};

#[pymodule]
pub fn turnip_text(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(experiment, m)?)?;

    // Primitives
    m.add_class::<BlockNode>()?;
    m.add_class::<InlineNode>()?;
    m.add_class::<UnescapedText>()?;
    m.add_class::<Sentence>()?;
    m.add_class::<Paragraph>()?;

    // Scopes
    m.add_class::<ScopeOwner>()?;
    m.add_class::<ImplicitScopedBlock>()?;
    m.add_class::<ExplicitScopedBlock>()?;
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
struct BlockNode {}

/// Parent class for objects representing content that stays within a single line/sentence.
#[pyclass(subclass)]
struct InlineNode {}

/// Represents plain inline text that has not yet been "escaped" for rendering.
///
/// Typically created by Rust while parsing input files.
#[pyclass(extends=InlineNode, subclass)]
struct UnescapedText(Py<PyString>);

/// A sequence of [InlineNode] that represents a single sentence.
///
/// Typically created by Rust while parsing input files.
#[pyclass(subclass)]
struct Sentence(Vec<Py<InlineNode>>);

/// A sequence of [Sentence] that combine to make a complete paragraph.
///
/// Typically created by Rust while parsing input files.
#[pyclass(extends=BlockNode, subclass)]
struct Paragraph(Vec<Py<Sentence>>);

/// Represents a block of plain text that may contain newlines (TODO are newlines normalized to \n?)
///
/// Typically created by Rust while parsing input files.
#[pyclass(extends=BlockNode, subclass)]
struct RawTextBlock(Py<PyString>);

/// A parent class (subclassed in Python) representing the "owner" of a scope,
/// which may modify how that scope is rendered.
#[pyclass(subclass)]
struct ScopeOwner {}

/// A block of [Paragraph]s and other [BlockNode]s, owned by a [ScopeOwner].
///
/// Implicitly created e.g. `[section("sectionref")]{Section Title}` automatically encompasses all following text until the next implicit block.
#[pyclass(extends=BlockNode, subclass)]
struct ImplicitScopedBlock {
    owner: Py<ScopeOwner>, // `[section("sectionref")]{Section Title}`
    children: Vec<Py<BlockNode>>,
    /// Used to determine what happens when another implicit block is encountered.
    /// If `self.level < next.level`, `next` is made a child of `self`.
    /// Otherwise it is considered the end of this implicit block, and it works up the stack (until the next explicit block).
    ///
    /// ex. `chapter.level = 0`, `section.level = 10`, `subsection.level = 20`... work it out from there.
    level: i64,
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
struct ExplicitScopedBlock {
    owner: Option<Py<ScopeOwner>>,
    children: Vec<Py<BlockNode>>,
}

/// A sequence of [UnescapedText]s and other [InlineNode]s, owned by a [ScopeOwner].
///
/// e.g. `[code]{this_is_formatted_as_code}`
#[pyclass(extends=InlineNode, subclass)]
struct InlineScope {
    owner: Option<Py<ScopeOwner>>,
    children: Vec<Py<InlineNode>>,
}
