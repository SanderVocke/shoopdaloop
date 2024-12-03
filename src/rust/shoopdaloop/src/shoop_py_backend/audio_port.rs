// See audio_port.rs from backend_bindings

use pyo3::prelude::*;
// use pyo3::exceptions::*;
use backend_bindings;

use crate::shoop_py_backend::backend_session::BackendSession;
use crate::shoop_py_backend::audio_driver::AudioDriver;

#[pyclass]
pub struct AudioPort {
    pub obj : backend_bindings::AudioPort,
}

#[pymethods]
impl AudioPort {
    #[getter]
    fn input_connectability(&self) -> PyResult<i32> {
        Ok(self.obj.input_connectability().to_ffi() as i32)
    }

    #[getter]
    fn output_connectability(&self) -> PyResult<i32> {
        Ok(self.obj.output_connectability().to_ffi() as i32)
    }

    #[getter]
    fn direction(&self) -> PyResult<i32> {
        Ok(self.obj.direction() as i32)
    }

    fn unsafe_backend_ptr (&self) -> usize {
        unsafe { self.obj.unsafe_backend_ptr() as usize }
    }
}

#[pyfunction]
pub fn open_driver_audio_port<'py>
    (backend_session : &BackendSession,
     audio_driver : &AudioDriver,
     name_hint : &str,
     direction : i32,
     min_n_ringbuffer_samples : u32) -> PyResult<AudioPort> {
    let dir = backend_bindings::PortDirection::try_from(direction).unwrap();
    Ok(AudioPort { obj: backend_bindings::AudioPort::new_driver_port
                           (&backend_session.obj,
                            &audio_driver.obj,
                            name_hint,
                            &dir,
                            min_n_ringbuffer_samples)
                                .unwrap() })
}

pub fn register_in_module<'py>(m: &Bound<'py, PyModule>) -> PyResult<()> {
    m.add_class::<AudioPort>()?;
    m.add_function(wrap_pyfunction!(open_driver_audio_port, m)?)?;
    Ok(())
}
