use pyo3::prelude::*;
use std::path::Path;
use shoop_rs_frontend;
use std::ffi::c_void;

#[pyfunction]
fn shoop_rust_init() {
    shoop_rs_frontend::init::shoop_rust_init();
}

#[pyfunction]
fn shoop_rust_create_autoconnect() -> u64 {
    unsafe {
        shoop_rs_frontend::init::shoop_rust_create_autoconnect() as usize as u64
    }
}

pub fn create_py_module<'py>(
    py : Python<'py>
) -> Result<Bound<'py, PyModule>, PyErr> {
    let m = PyModule::new_bound(py, "shoop_rust")?;
    m.add_function(wrap_pyfunction!(shoop_rust_init, &m)?)?;
    m.add_function(wrap_pyfunction!(shoop_rust_create_autoconnect, &m)?)?;
    Ok(m)
}