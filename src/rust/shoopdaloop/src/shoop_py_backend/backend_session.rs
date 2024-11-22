// See backend_session.rs from backend_bindings

use pyo3::prelude::*;
// use pyo3::exceptions::*;
use backend_bindings;
use crate::shoop_py_backend::audio_driver::AudioDriver;

#[pyclass]
pub struct BackendSession {
    pub obj : backend_bindings::BackendSession,
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

    fn set_audio_driver(&self, driver : &AudioDriver) -> PyResult<()> {
        if self.obj.set_audio_driver(&driver.obj).is_ok() {
            Ok(())
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyException, _>("set_audio_driver() failed"))
        }
    }
}

pub fn register_in_module<'py>(m: &Bound<'py, PyModule>) -> PyResult<()> {
    m.add_class::<BackendSession>()?;
    Ok(())
}