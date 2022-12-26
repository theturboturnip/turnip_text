use std::ffi::CStr;

use pyembed::{ExtensionModule, MainPythonInterpreter, OxidizedPythonInterpreterConfig};
use pyo3::prelude::*;

#[pyfunction]
fn experiment() -> PyResult<usize> {
    eprintln!("called experiment");
    Ok(42)
}
#[pymodule]
fn turnip_text(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(experiment, m)?)?;
    Ok(())
}

/// Struct holding references to current Python state, including the relevant globals/locals.
/// Once created, the GIL is held until it is [Drop]-ped
pub struct TurnipTextPython<'py> {
    pub interp: MainPythonInterpreter<'py, 'py>,
}

fn interpreter_config<'a>() -> OxidizedPythonInterpreterConfig<'a> {
    let mut base_config = OxidizedPythonInterpreterConfig::default();
    // Clear argv - our command-line arguments are not useful for the embedded python
    base_config.argv = Some(vec![]);
    base_config.extra_extension_modules = Some(vec![ExtensionModule {
        name: CStr::from_bytes_with_nul(turnip_text::NAME.as_bytes())
            .unwrap()
            .to_owned(),
        init_func: turnip_text::init,
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
