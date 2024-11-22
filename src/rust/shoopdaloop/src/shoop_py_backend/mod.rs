use pyo3::prelude::*;
mod resample;
mod audio_driver;
mod backend_session;

pub fn create_py_module<'py>(
    py : Python<'py>
) -> Result<Bound<'py, PyModule>, PyErr> {
    let m = PyModule::new_bound(py, "shoop_py_backend")?;
    resample::register_in_module(&m)?;
    audio_driver::register_in_module(&m)?;
    backend_session::register_in_module(&m)?;
    Ok(m)
}