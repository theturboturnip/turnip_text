use pyo3::{types::PyDict, PyResult};
use turnip_text::python::TurnipTextPython;

fn main() {
    eprintln!("Started test!!!");

    let ttpy = TurnipTextPython::new();
    eprintln!("Created TurnipTextPython");
    pyo3::prepare_freethreaded_python();
    let res = ttpy.interp.with_gil(|py| -> PyResult<usize> {
        eprintln!("interpreter got GIL");

        let globals = PyDict::new(py);
        let locals = PyDict::new(py);
        py.eval("5+7", Some(&globals), Some(&locals)).unwrap();
        eprintln!("eval 5+7 success");

        py.run("import json", Some(&globals), Some(&locals))
            .unwrap();
        eprintln!("eval import json success");

        py.run("import turnip_text", Some(&globals), Some(&locals))
            .unwrap();
        eprintln!("eval import turnip_text success");

        let code = "turnip_text.experiment()";
        let experiment_val: usize = py.eval(code, Some(&globals), Some(&locals))?.extract()?;
        eprintln!("eval turnip_text.experiment() success");
        Ok(experiment_val)
    });
    assert_eq!(res.unwrap(), 42);
}
