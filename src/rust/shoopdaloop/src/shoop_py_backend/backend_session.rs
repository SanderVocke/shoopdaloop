// See audio_driver.rs from backend_bindings

use pyo3::prelude::*;
// use pyo3::exceptions::*;
use backend_bindings;

#[pyclass]
pub struct BackendSession {
    obj : backend_bindings::BackendSession,
}

#[pymethods]
impl BackendSession {
    #[new]
    fn py_new() -> PyResult<Self> {
        Ok(BackendSession { obj: backend_bindings::BackendSession::new().unwrap() })
    }

    fn unsafe_backend_ptr (&self) -> usize {
        unsafe { self.obj.unsafe_backend_ptr() as usize }
    }
}

pub fn register_in_module<'py>(m: &Bound<'py, PyModule>) -> PyResult<()> {
    m.add_class::<BackendSession>()?;
    Ok(())
}