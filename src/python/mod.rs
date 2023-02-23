use std::ffi::CStr;

use pyembed::{ExtensionModule, MainPythonInterpreter, OxidizedPythonInterpreterConfig};
use pyo3::{prelude::*, types::PyDict};

use crate::lexer::TTToken;

pub mod interop;
use interop::turnip_text;

mod interp;
use self::{interop::BlockScope, interp::InterpState};

pub mod typeclass;

/// Utility struct for custom interpreter
pub struct TurnipTextPython<'interp> {
    pub interp: MainPythonInterpreter<'interp, 'interp>,
}
// For testing purposes
unsafe impl<'interp> Send for TurnipTextPython<'interp> {}

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
    base_config.origin = None;
    base_config
}

impl<'interp> TurnipTextPython<'interp> {
    pub fn new() -> TurnipTextPython<'interp> {
        // TODO can use append_to_inittab to get past all of this!!!!!
        let interp = MainPythonInterpreter::new(interpreter_config())
            .expect("Couldn't create python interpreter");

        pyo3::prepare_freethreaded_python();

        Self { interp }
    }

    pub fn with_gil<F, R>(&self, f: F) -> R
    where
        F: for<'py> FnOnce(Python<'py>) -> R,
    {
        self.interp.with_gil(f)
    }
}

pub use interp::{InterpError, InterpResult};
pub fn interp_data(
    py: Python,
    globals: &PyDict,
    data: &str,
    toks: impl Iterator<Item = TTToken>,
) -> InterpResult<Py<BlockScope>> {
    let mut st = InterpState::new(py, data)?;
    let res: InterpResult<()> = toks.map(|t| st.handle_token(py, globals, t)).collect();
    res?;
    st.finalize(py, globals)?;
    Ok(st.root())
}
