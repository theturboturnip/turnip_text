use anyhow::bail;
use argh::FromArgs;
use pyo3::{types::PyModule, PyResult, Python};
use turnip_text::python::prepare_freethreaded_turniptext_python;

#[derive(FromArgs)]
#[argh(description = "")]
struct ParseCmd {
    #[argh(positional)]
    path: std::path::PathBuf,
}

fn main() -> anyhow::Result<()> {
    let args: ParseCmd = argh::from_env();
    let py_file = std::fs::read_to_string(args.path)?;
    prepare_freethreaded_turniptext_python();
    let res = Python::with_gil(|py| -> PyResult<()> {
        PyModule::from_code(py, &py_file, "", "__main__")?;
        Ok(())
    });
    match res {
        Err(py_err) => {
            bail!(py_err.to_string())
        }
        _ => Ok(()),
    }
}
