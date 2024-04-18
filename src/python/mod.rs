use pyo3::{
    exceptions::PySyntaxError, ffi::Py_None, intern, types::PyDict, Py, PyAny, PyResult, Python,
};
use {once_cell::sync::Lazy, regex::Regex};

pub mod interop;

pub mod typeclass;

/// Prepare an embedded Python interpreter with our module.
///
/// Not valid when included as an extension module, but used when running Rust directly e.g. in testing.
#[cfg(not(feature = "extension-module"))]
pub fn prepare_freethreaded_turniptext_python() {
    use interop::turnip_text;
    pyo3::append_to_inittab!(turnip_text);
    pyo3::prepare_freethreaded_python();
}

/// Given a string, check if the first line with non-whitespace content has an indent
/// (i.e. whitespace between the content and the start of the line).
fn has_initial_indent(code: &str) -> bool {
    // That translates into the following regex:
    // ^                     -- start from the start of the text
    //  (\s*\n)*             -- greedily capture empty lines
    //          (\s+)        -- capture nonzero indent from the start of the line
    //               [^\s]   -- find non-whitespace content
    static INDENT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(\s*\n)*(\s+)[^\s]").unwrap());
    INDENT_RE.is_match(code)
}

pub fn eval_or_exec<'py, 'code>(
    py: Python<'py>,
    py_env: &'py PyDict,
    code: &'code str,
) -> PyResult<&'py PyAny> {
    // Python picks up leading whitespace as an incorrect indent.
    let code = code.trim();
    // TODO in multiline contexts it would be really nice to allow a toplevel indent (ignoring blank lines when calculating it)
    let raw_res = match py.eval(code, Some(py_env), None) {
        Ok(raw_res) => raw_res,
        Err(error) if error.is_instance_of::<PySyntaxError>(py) => {
            // Try to exec() it as a statement instead of eval() it as an expression

            // TODO WAIT YOU CANT DO THIS BY REGEX BECAUSE COMMENTS DONT COUNT, AND I DONT WANT TO PARSE PYTHON THAT HARD
            // Indent-enabling: scan through the string until you reach a line with content, measure the indent of that content, and if it's nonzero we append `if True:\n` at the front of the string.
            if test_code_has_initial_indent(code) {
                let mut safe_code = "if True:\n".to_owned();
                safe_code.push_str(code);
                py.run(&safe_code, Some(py_env), None)?;
            } else {
                // Zero-indent, run without an if True.
                py.run(code, Some(py_env), None)?;
            }

            // exec() for us returns None.
            // Acquire a Py<PyAny> to Python None, then use into_ref() to convert it into a &PyAny.
            // This should optimize down to `*Py_None()` because Py<T> and PyAny both boil down to *ffi::Py_Object;
            // This is so that places that *require* non-None (e.g. NeedBlockBuilder) will always raise an error in the following match statement.
            // This is safe because Py_None() returns a pointer-to-static.
            unsafe { Py::<PyAny>::from_borrowed_ptr(py, Py_None()).into_ref(py) }
        }
        Err(error) => return Err(error),
    };
    // If it has __get__, call it.
    // `property` objects and other data descriptors use this.
    let getter = intern!(py, "__get__");
    if raw_res.hasattr(getter)? {
        // https://docs.python.org/3.8/howto/descriptor.html
        // "For objects, the machinery is in object.__getattribute__() which transforms b.x into type(b).__dict__['x'].__get__(b, type(b))."
        //
        // We're transforming `[x]` into (effectively) `py_env.x`
        // which should transform into (type(py_env).__dict__['x']).__get__(py_env, type(py_env))
        // = raw_res.__get__(py_env, type(py_env))
        raw_res.call_method1(getter, (py_env, py_env.get_type()))
    } else {
        Ok(raw_res)
    }
}

#[cfg(test)]
mod test {
    use super::has_initial_indent;

    fn assert_indent(code: &str) {
        if !has_initial_indent(code) {
            dbg!(code);
            panic!("has_initial_indent() returned False");
        }
    }

    fn assert_no_indent(code: &str) {
        if has_initial_indent(code) {
            dbg!(code);
            panic!("has_initial_indent() returned True");
        }
    }

    #[test]
    fn test_unindented_examples() {
        assert_no_indent("no indent here");
        assert_no_indent("\nno indent here");
        assert_no_indent("   \n     \t\t  \nno indent here");
    }
}
