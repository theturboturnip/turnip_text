use argh::FromArgs;
use pyo3::{types::PyModule, PyResult};
use turnip_text::python::TurnipTextPython;

#[derive(FromArgs)]
#[argh(description = "")]
struct ParseCmd {
    #[argh(positional)]
    path: std::path::PathBuf,
}

fn main() -> anyhow::Result<()> {
    let args: ParseCmd = argh::from_env();
    let py_file = std::fs::read_to_string(args.path)?;
    let ttpython = TurnipTextPython::new();
    ttpython.with_gil(|py| -> PyResult<()> {
        PyModule::from_code(py, &py_file, "", "__main__")?;
        Ok(())
    })?;
    Ok(())
}
