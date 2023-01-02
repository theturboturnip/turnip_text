use std::ffi::CStr;

use pyembed::{ExtensionModule, MainPythonInterpreter, OxidizedPythonInterpreterConfig};
use pyo3::{prelude::*, types::PyDict};

pub mod interop;
use interop::turnip_text;

use crate::lexer::TTToken;

use self::{interp::{InterpState, InterpResult}, interop::BlockScope};

mod interp;
mod typeclass;

/// Struct holding references to current Python state, including the relevant globals/locals.
pub struct TurnipTextPython<'interp> {
    pub interp: MainPythonInterpreter<'interp, 'interp>,
    pub globals: Py<PyDict>,
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

impl<'interp> TurnipTextPython<'interp> {
    pub fn new() -> TurnipTextPython<'interp> {
        let interp = MainPythonInterpreter::new(interpreter_config())
            .expect("Couldn't create python interpreter");

        pyo3::prepare_freethreaded_python();
        let globals = interp
            .with_gil(|py| -> PyResult<Py<PyDict>> {
                let globals = PyDict::new(py);
                py.run("from turnip_text import *", Some(globals), None)?;
                Ok(globals.into())
            })
            .unwrap();

        Self { interp, globals }
    }

    pub fn with_gil<F, R>(&self, f: F) -> R
    where
        F: for<'py> FnOnce(Python<'py>, &'py PyDict) -> R,
    {
        self.interp
            .with_gil(|py| -> R { f(py, self.globals.as_ref(py)) })
    }
}

pub use interp::InterpError;
pub fn interp_data(
    ttpython: &TurnipTextPython<'_>,
    data: &str,
    toks: impl Iterator<Item = TTToken>
) -> InterpResult<Py<BlockScope>> {
    let mut st = InterpState::new(ttpython, data)?;
    let res: InterpResult<()> = toks
        .map(|t| st.handle_token(ttpython, t))
        .collect();
    res?;
    Ok(st.root())
} 