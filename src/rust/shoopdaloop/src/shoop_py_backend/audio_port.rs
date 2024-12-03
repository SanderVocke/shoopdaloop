// See audio_port.rs from backend_bindings

use pyo3::prelude::*;
// use pyo3::exceptions::*;
use backend_bindings;

use crate::shoop_py_backend::backend_session::BackendSession;
use backend_bindings::AudioPortState as BackendAudioPortState;
use crate::shoop_py_backend::audio_driver::AudioDriver;
use std::collections::HashMap;


#[pyclass]
pub struct AudioPortState {
    #[pyo3(get)]
    pub input_peak: f32,
    #[pyo3(get)]
    pub output_peak: f32,
    #[pyo3(get)]
    pub gain: f32,
    #[pyo3(get)]
    pub muted: u32,
    #[pyo3(get)]
    pub passthrough_muted: u32,
    #[pyo3(get)]
    pub ringbuffer_n_samples: u32,
    #[pyo3(get)]
    pub name: String,
}

impl AudioPortState {
    pub fn new(state: BackendAudioPortState) -> Self {
        AudioPortState {
            input_peak: state.input_peak,
            output_peak: state.output_peak,
            gain: state.gain,
            muted: state.muted,
            passthrough_muted: state.passthrough_muted,
            ringbuffer_n_samples: state.ringbuffer_n_samples,
            name: state.name,
        }
    }
}

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

    fn get_state(&self) -> PyResult<AudioPortState> {
        Ok(AudioPortState::new(self.obj.get_state()))
    }

    fn set_gain(&self, gain: f32) -> PyResult<()> {
        self.obj.set_gain(gain);
        Ok(())
    }

    fn set_muted(&self, muted: bool) -> PyResult<()> {
        self.obj.set_muted(muted);
        Ok(())
    }

    fn set_passthrough_muted(&self, muted: bool) -> PyResult<()> {
        self.obj.set_passthrough_muted(muted);
        Ok(())
    }

    fn connect_internal(&self, other: &AudioPort) -> PyResult<()> {
        self.obj.connect_internal(&other.obj);
        Ok(())
    }

    fn dummy_queue_data(&self, data: Vec<f32>) -> PyResult<()> {
        self.obj.dummy_queue_data(&data);
        Ok(())
    }

    fn dummy_dequeue_data(&self, n_samples: u32) -> PyResult<Vec<f32>> {
        Ok(self.obj.dummy_dequeue_data(n_samples))
    }

    fn dummy_request_data(&self, n_samples: u32) -> PyResult<()> {
        self.obj.dummy_request_data(n_samples);
        Ok(())
    }

    fn get_connections_state(&self) -> PyResult<HashMap<String, bool>> {
        Ok(self.obj.get_connections_state())
    }

    fn connect_external_port(&self, name: &str) -> PyResult<()> {
        self.obj.connect_external_port(name);
        Ok(())
    }

    fn disconnect_external_port(&self, name: &str) -> PyResult<()> {
        self.obj.disconnect_external_port(name);
        Ok(())
    }

    fn set_ringbuffer_n_samples(&self, n: u32) -> PyResult<()> {
        self.obj.set_ringbuffer_n_samples(n);
        Ok(())
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
