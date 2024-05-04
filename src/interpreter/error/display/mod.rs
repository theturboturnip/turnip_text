mod annotate_snippets;
mod codespan;

use pyo3::Python;

use super::TTErrorWithContext;

pub fn detailed_message_of(py: Python, err: &TTErrorWithContext) -> String {
    // annotate_snippets::detailed_message_of(err)
    codespan::detailed_message_of(py, err)
}
