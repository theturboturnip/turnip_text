use pyo3::{types::PyDict, PyResult, Python};
use turnip_text::python::prepare_freethreaded_turniptext_python;

fn main() {
    eprintln!("Started test!!!");

    prepare_freethreaded_turniptext_python();
    eprintln!("Inited Python with turnip_text");
    Python::with_gil(|py| -> PyResult<()> {
        eprintln!("interpreter got GIL");

        let locals = PyDict::new(py);
        py.eval("5+7", None, Some(&locals)).unwrap();
        eprintln!("eval 5+7 success");

        py.run("import json", None, Some(&locals)).unwrap();
        eprintln!("run import json success");

        py.run("import turniptext", None, Some(&locals)).unwrap();
        eprintln!("run import turniptext success");
        Ok(())
    })
    .unwrap();
}
