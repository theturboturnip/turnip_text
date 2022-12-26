use pyo3::{types::PyDict, PyResult};
use turnip_text::python::TurnipTextPython;

fn main() {
    eprintln!("Started test!!!");

    let ttpy = TurnipTextPython::new();
    eprintln!("Created TurnipTextPython");
    let res = ttpy.with_gil(|py, globals| -> PyResult<usize> {
        eprintln!("interpreter got GIL");

        let locals = PyDict::new(py);
        py.eval("5+7", Some(&globals), Some(&locals)).unwrap();
        eprintln!("eval 5+7 success");

        py.run("import json", Some(&globals), Some(&locals))
            .unwrap();
        eprintln!("run import json success");

        let code = "experiment()";
        let experiment_val: usize = py.eval(code, Some(&globals), Some(&locals))?.extract()?;
        eprintln!("eval experiment() success");
        Ok(experiment_val)
    });
    assert_eq!(res.unwrap(), 42);
}
