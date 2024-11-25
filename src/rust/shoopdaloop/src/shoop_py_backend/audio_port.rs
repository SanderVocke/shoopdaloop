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

#[pyfunction]
pub fn unsafe_audio_port_from_raw_ptr<'py>(ptr : u32) -> PyResult<AudioPort> {
    Ok(AudioPort { obj: backend_bindings::AudioPort::unsafe_port_from_raw_ptr(ptr) })
}

pub fn register_in_module<'py>(m: &Bound<'py, PyModule>) -> PyResult<()> {
    m.add_class::<AudioPort>()?;
    m.add_function(wrap_pyfunction!(open_driver_audio_port, m)?)?;
    m.add_function(wrap_pyfunction!(unsafe_audio_port_from_raw_ptr, m)?)?;
    Ok(())
}