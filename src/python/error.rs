//! Python errors have `__cause__` and `__context__`.
//! `__context__` is automatically set when raising an exception inside an `except` in Python:
//!
//! ```python
//! try:
//!     raise ValueError()
//! except ValueError:
//!     some_dict = {}
//!     some_dict['y'] # raises a KeyError, which has __context__ set to the ValueError() automatically
//! ```
//!
//! `__cause__` is set when explicitly raising an exception from another exception:
//!
//!
//! ```python
//! try:
//!     raise ValueError()
//! except ValueError as v:
//!     # has __context__ set to the ValueError() automatically
//!     # has __cause__ set explicitly to the ValueError().
//!     raise OtherError() from v
//! ```
//!
//! When printing tracebacks, if `__cause__` is set then both the toplevel exception and `__cause__` are printed.
//! This also applies if `__cause__` has a `__cause__`, and if `__cause__` isn't set then Python will look in `__context__` too.
//! Having `__cause__` and `__context__` for errors is a useful debugging tool.
//!
//! This module provides functions for setting context/cause as an abstraction over the ever-moving PyO3 API.
//!
//! Sources:
//! - https://docs.python.org/3/library/exceptions.html#exception-context demonstrating cause and context
//! - https://stackoverflow.com/a/51074769 on manipulating cause and context in C

use pyo3::prelude::*;

fn set_context(py: Python, new_err: &mut PyErr, context: PyErr) {
    // Based on pyo3::err::PyErr::set_cause() source code as of PyO3 0.21.2
    let value = new_err.value_bound(py);
    let context = context.into_value(py);
    unsafe {
        // PyException_SetContext _steals_ a reference to cause, so must use .into_ptr()
        // https://github.com/python/cpython/blob/13245027526bf1b21fae6e7ca62ceec2e39fcfb7/Objects/exceptions.c#L416
        pyo3::ffi::PyException_SetContext(value.as_ptr(), Py::into_ptr(context));
    }
}

fn set_cause(py: Python, new_err: &mut PyErr, cause: PyErr) {
    new_err.set_cause(py, Some(cause))
}

/// The equivalent to `raise new_err from cause` in Python.
pub fn set_cause_and_context(py: Python, new_err: &mut PyErr, cause: PyErr) {
    // Both set_context and set_clause require distinct references to exception values,
    // because the C API functions *steal* those references.
    set_context(py, new_err, cause.clone_ref(py));
    set_cause(py, new_err, cause);
}
