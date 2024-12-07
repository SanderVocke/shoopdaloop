// See audio_port.rs from backend_bindings

use pyo3::prelude::*;
// use pyo3::exceptions::*;
use backend_bindings;

use crate::shoop_py_backend::backend_session::BackendSession;
use crate::shoop_py_backend::audio_driver::AudioDriver;
use crate::shoop_py_backend::midi::MidiEvent;

use std::collections::HashMap;

#[pyclass]
pub struct MidiPortState {
    #[pyo3(get)]
    pub n_input_events: u32,
    #[pyo3(get)]
    pub n_input_notes_active: u32,
    #[pyo3(get)]
    pub n_output_events: u32,
    #[pyo3(get)]
    pub n_output_notes_active: u32,
    #[pyo3(get)]
    pub muted: u32,
    #[pyo3(get)]
    pub passthrough_muted: u32,
    #[pyo3(get)]
    pub ringbuffer_n_samples: u32,
    #[pyo3(get)]
    pub name: String,
}

impl MidiPortState {
    pub fn new(state: backend_bindings::MidiPortState) -> Self {
        MidiPortState {
            n_input_events: state.n_input_events,
            n_input_notes_active: state.n_input_notes_active,
            n_output_events: state.n_output_events,
            n_output_notes_active: state.n_output_notes_active,
            muted: state.muted,
            passthrough_muted: state.passthrough_muted,
            ringbuffer_n_samples: state.ringbuffer_n_samples,
            name: state.name,
        }
    }
}

#[pyclass]
pub struct MidiPort {
    pub obj : backend_bindings::MidiPort,
}

#[pymethods]
impl MidiPort {
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
    
    fn get_state(&self) -> PyResult<MidiPortState> {
        let state = self.obj.get_state();
        Ok(MidiPortState::new(state))
    }

    fn set_muted(&self, muted: bool) -> PyResult<()> {
        self.obj.set_muted(muted);
        Ok(())
    }

    fn set_passthrough_muted(&self, muted: bool) -> PyResult<()> {
        self.obj.set_passthrough_muted(muted);
        Ok(())
    }

    fn connect_internal(&self, other: &MidiPort) -> PyResult<()> {
        self.obj.connect_internal(&other.obj);
        Ok(())
    }

    fn dummy_clear_queues(&self) -> PyResult<()> {
        self.obj.dummy_clear_queues();
        Ok(())
    }

    fn dummy_queue_msg(&self, msg: &MidiEvent) -> PyResult<()> {
        self.obj.dummy_queue_msg(&msg.to_backend());
        Ok(())
    }

    fn dummy_queue_msgs(&self, msgs: Vec<MidiEvent>) -> PyResult<()> {
        let converted = msgs.iter().map(|msg| msg.to_backend()).collect();
        self.obj.dummy_queue_msgs(converted);
        Ok(())
    }

    fn dummy_dequeue_data(&self) -> PyResult<Vec<MidiEvent>> {
        let msgs = self.obj.dummy_dequeue_data();
        Ok(msgs.into_iter().map(|msg| MidiEvent::from(msg)).collect())
    }

    fn dummy_request_data(&self, n_frames: u32) -> PyResult<()> {
        self.obj.dummy_request_data(n_frames);
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

    fn unsafe_backend_ptr(&self) -> usize {
        unsafe { self.obj.unsafe_backend_ptr() as usize }
    }
}

#[pyfunction]
pub fn open_driver_midi_port<'py>
    (backend_session : &BackendSession,
     audio_driver : &AudioDriver,
     name_hint : &str,
     direction : i32,
     min_n_ringbuffer_samples : u32) -> PyResult<MidiPort> {
    let dir = backend_bindings::PortDirection::try_from(direction).unwrap();
    Ok(MidiPort { obj: backend_bindings::MidiPort::new_driver_port
                           (&backend_session.obj,
                            &audio_driver.obj,
                            name_hint,
                            &dir,
                            min_n_ringbuffer_samples)
                                .unwrap() })
}

pub fn register_in_module<'py>(m: &Bound<'py, PyModule>) -> PyResult<()> {
    m.add_class::<MidiPort>()?;
    m.add_class::<MidiPortState>()?;
    m.add_function(wrap_pyfunction!(open_driver_midi_port, m)?)?;
    Ok(())
}
