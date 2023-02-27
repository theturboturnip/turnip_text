use pyo3::{prelude::*, types::PyDict};

use crate::lexer::TTToken;

pub mod interop;
use interop::turnip_text;

mod interp;
use self::{interop::BlockScope, interp::InterpState};

pub mod typeclass;

pub struct TurnipTextPython {}

impl TurnipTextPython {
    pub fn new() -> Self {
        pyo3::append_to_inittab!(turnip_text);
        pyo3::prepare_freethreaded_python();

        Self {}
    }

    pub fn with_gil<F, R>(&self, f: F) -> R
    where
        F: for<'py> FnOnce(Python<'py>) -> R,
    {
        Python::with_gil(f)
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
