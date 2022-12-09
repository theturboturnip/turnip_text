use std::ffi::CString;

use pyembed::{ExtensionModule, MainPythonInterpreter, OxidizedPythonInterpreterConfig};
use pyo3::{
    ffi::{
        PyErr_Occurred, PyLong_FromLong, PyMethodDef, PyModuleDef, PyModuleDef_HEAD_INIT,
        PyModule_Create, Py_INCREF, METH_VARARGS,
    },
    prelude::*,
};

#[pyfunction]
fn experiment_pyo3() -> PyResult<usize> {
    eprintln!("called experiment_pyo3");
    Ok(42)
}

unsafe extern "C" fn experiment(
    s: *mut pyo3::ffi::PyObject,
    args: *mut pyo3::ffi::PyObject,
) -> *mut pyo3::ffi::PyObject {
    eprintln!("called experiment()");
    PyLong_FromLong(42)
}

/// Struct holding references to current Python state, including the relevant globals/locals.
/// Once created, the GIL is held until it is [Drop]-ped
pub struct TurnipTextPython<'py> {
    pub interp: MainPythonInterpreter<'py, 'py>,
}

static MODULE_NAME: &str = "turnip_text\0";
static MODULE_DOCSTRING: &str = "turnip_text docstring\0";
static mut MODULE_DEF: PyModuleDef = PyModuleDef {
    m_base: PyModuleDef_HEAD_INIT,
    m_name: MODULE_NAME.as_ptr() as *const i8,
    m_doc: MODULE_DOCSTRING.as_ptr() as *const i8,
    m_size: -1,
    m_methods: unsafe { FUNCS.as_mut_ptr() },
    m_slots: std::ptr::null_mut(),
    m_traverse: None,
    m_clear: None,
    m_free: None,
};

// We can't use the PyO3 generated PyMethodDef, because it's all pub(crate) @?%#!
static EXPERIMENT_NAME: &str = "experiment\0";
static EXPERIMENT_DOCSTRING: &str = "experiment docstring\0";
static mut FUNCS: [PyMethodDef; 2] = [
    PyMethodDef {
        ml_name: EXPERIMENT_NAME.as_ptr() as *const i8,
        ml_meth: pyo3::ffi::PyMethodDefPointer {
            PyCFunction: experiment,
        },
        ml_flags: METH_VARARGS,
        ml_doc: EXPERIMENT_DOCSTRING.as_ptr() as *const i8,
    },
    PyMethodDef::zeroed(),
]; //[experiment::DEF.as_method_def(), PyMethodDef::zeroed()];

#[allow(non_snake_case)]
unsafe extern "C" fn PyInit_turnip_text() -> *mut pyo3::ffi::PyObject {
    // Python::with_gil(|py| -> *mut pyo3::ffi::PyObject {
    //     eprintln!("Got GIL");
    //     if PyErr_Occurred() != std::ptr::null_mut() {
    //         panic!("Error occurred getting gil")
    //     }
    //     let m = PyModule::new(py, "turnip_text").expect("Failed to create PyModule");
    //     if PyErr_Occurred() != std::ptr::null_mut() {
    //         panic!("Error occurred creating pymodule")
    //     }
    //     let func = wrap_pyfunction!(experiment_pyo3, m).expect("failed to wrap pyfunction");
    //     if PyErr_Occurred() != std::ptr::null_mut() {
    //         panic!("Error occurred wrapping func")
    //     }
    //     Py_INCREF(func.into_ptr());
    //     m.add_function(func).expect("failed to add function");
    //     if PyErr_Occurred() != std::ptr::null_mut() {
    //         panic!("Error occurred adding func")
    //     }
    //     let ptr = m.into_ptr();
    //     Py_INCREF(ptr);
    //     if PyErr_Occurred() != std::ptr::null_mut() {
    //         panic!("Error occurred returning ptr")
    //     }
    //     dbg!(ptr)
    // })

    eprintln!("PyInit_turnip_text called");
    PyModule_Create(&mut MODULE_DEF)
}

fn interpreter_config<'a>() -> OxidizedPythonInterpreterConfig<'a> {
    let mut base_config = OxidizedPythonInterpreterConfig::default();
    // Clear argv - our command-line arguments are not useful for the embedded python
    base_config.argv = Some(vec![]);
    base_config.extra_extension_modules = Some(vec![ExtensionModule {
        name: CString::new("turnip_text").unwrap(),
        init_func: PyInit_turnip_text,
    }]);
    // "If this is false, the default path configuration built into libpython is used."
    // This avoids a `init_fs_encoding` error message, where python tries to import the standard library and fails because we've told it the stdlib is installed relative to the executable
    base_config.set_missing_path_configuration = false;
    base_config
}

impl<'py> TurnipTextPython<'py> {
    pub fn new() -> TurnipTextPython<'py> {
        let interp = MainPythonInterpreter::new(interpreter_config())
            .expect("Couldn't create python interpreter");

        Self { interp }
    }
}
