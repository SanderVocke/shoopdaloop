use pyo3::prelude::*;
mod resample;

pub fn create_py_module<'py>(
    py : Python<'py>
) -> Result<Bound<'py, PyModule>, PyErr> {
    let m = PyModule::new_bound(py, "shoop_py_backend")?;
    m.add_class::<resample::MultichannelAudio>()?;
    Ok(m)
}