// use pyembed::{
//     ExtensionModule, NewInterpreterError, OxidizedPythonInterpreterConfig,
//     ResolvedOxidizedPythonInterpreterConfig,
// };
use pyo3::prelude::*;
// use std::ffi::CString;

#[pyfunction]
fn experiment() -> PyResult<usize> {
    Ok(42)
}

// unsafe extern "C" fn PyInit_turnip_text() -> *mut pyo3::ffi::PyObject {
//     // let module = PyModule::new(py, "turnip_text")?;
//     // module.add_function(wrap_pyfunction!(experiment, m)?)?;
//     // module.into_ptr()
//     todo!()
// }

// fn python_config<'a>() -> Result<ResolvedOxidizedPythonInterpreterConfig<'a>, NewInterpreterError> {
//     let mut base_config = OxidizedPythonInterpreterConfig::default();
//     // Clear argv - our command-line arguments are not useful for the embedded python
//     base_config.argv = Some(vec![]);
//     base_config.extra_extension_modules = Some(vec![ExtensionModule {
//         name: CString::new("turnip_text").unwrap(),
//         init_func: PyInit_turnip_text,
//     }]);
//     base_config.resolve()
// }

#[cfg(test)]
#[test]
fn test_python() -> PyResult<()> {
    use pyo3::types::IntoPyDict;

    let res = Python::with_gil(|py| -> PyResult<usize> {
        let module = PyModule::new(py, "turnip_text")?;
        module.add_function(wrap_pyfunction!(experiment, module)?)?;

        // TODO put turnip_text in globals without breaking what's already there
        let locals = [("turnip_text", module)].into_py_dict(py);
        let code = "turnip_text.experiment()";
        let experiment_val: usize = py.eval(code, None, Some(&locals))?.extract()?;

        Ok(experiment_val)
    });
    assert_eq!(res?, 42);
    Ok(())
}
