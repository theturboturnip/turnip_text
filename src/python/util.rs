use pyo3::{intern, prelude::*};

/// Get the `__name__` attribute of obj, stringify it, and return it
pub fn get_name(obj: &Bound<'_, PyAny>) -> Option<String> {
    if let Ok(name) = obj.getattr(intern!(obj.py(), "__name__")) {
        if !name.is_none() {
            return Some(stringify_py(&name));
        }
    }
    None
}

/// Get the `__qualname__` attribute of obj, stringify it, and return it
pub fn get_qualname(obj: &Bound<'_, PyAny>) -> Option<String> {
    if let Ok(qname) = obj.getattr(intern!(obj.py(), "__qualname__")) {
        if !qname.is_none() {
            return Some(stringify_py(&qname));
        }
    }
    None
}

/// Get the `__doc__` attribute of obj, stringify it, and return it
pub fn get_docstring(obj: &Bound<'_, PyAny>) -> Option<String> {
    if let Ok(doc) = obj.getattr(intern!(obj.py(), "__doc__")) {
        if !doc.is_none() {
            return Some(stringify_py(&doc));
        }
    }
    None
}

/// Call `__str__()` on the obj, stringify that, and return it
pub fn stringify_py(obj: &Bound<'_, PyAny>) -> String {
    obj.str()
        .map_or("<stringification failed>".into(), |pystring| {
            pystring.to_string()
        })
}
