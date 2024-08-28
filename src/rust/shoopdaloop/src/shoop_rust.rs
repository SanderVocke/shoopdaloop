use pyo3::prelude::*;
use frontend;

#[pyfunction]
fn shoop_rust_init() {
    frontend::init::shoop_rust_init();
}

#[pyfunction]
fn shoop_rust_create_autoconnect() -> u64 {
    unsafe {
        frontend::init::shoop_rust_create_autoconnect() as usize as u64
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