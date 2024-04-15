use pyo3::{
    exceptions::PySyntaxError, ffi::Py_None, intern, types::PyDict, Py, PyAny, PyResult, Python,
};

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
            py.run(code, Some(py_env), None)?;
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
