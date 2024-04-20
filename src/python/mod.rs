pub mod interop;
pub mod typeclass;

/// Prepare an embedded Python interpreter with our module.
///
/// Not valid when included as an extension module, but used when running Rust directly e.g. in testing.
#[cfg(not(feature = "extension-module"))]
pub fn prepare_freethreaded_turniptext_python() {
    use interop::turnip_text;
    pyo3::append_to_inittab!(turnip_text);
    pyo3::prepare_freethreaded_python();
}
