use pyo3::prelude::*;

#[pymodule]
pub fn turnip_text(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(experiment, m)?)?;
    Ok(())
}

#[pyfunction]
fn experiment() -> PyResult<usize> {
    eprintln!("called experiment");
    Ok(42)
}
