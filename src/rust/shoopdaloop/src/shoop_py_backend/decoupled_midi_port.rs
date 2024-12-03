// See audio_port.rs from backend_bindings

use pyo3::prelude::*;
use pyo3::exceptions::PyValueError;

use crate::shoop_py_backend::midi::MidiEvent;
use crate::shoop_py_backend::audio_driver::AudioDriver;

#[pyclass]
pub struct DecoupledMidiPort {
    pub obj : backend_bindings::DecoupledMidiPort,
}

#[pymethods]
impl DecoupledMidiPort {
    fn unsafe_backend_ptr (&self) -> usize {
        unsafe { self.obj.unsafe_backend_ptr() as usize }
    }

    fn maybe_next_message(&self) -> Option<MidiEvent> {
        match self.obj.maybe_next_message() {
            None => None,
            Some(event) => Some(MidiEvent::from(event))
        }
    }

    fn name(&self) -> String {
        self.obj.name()
    }

    fn send_midi(&self, msg_bytes: &[u8]) -> PyResult<()> {
        if msg_bytes.is_empty() {
            return Err(PyValueError::new_err("MIDI message cannot be empty"));
        }
        self.obj.send_midi(msg_bytes);
        Ok(())
    }

    fn get_connections_state(&self) -> std::collections::HashMap<String, bool> {
        self.obj.get_connections_state()
    }

    fn connect_external_port(&self, name: &str) {
        self.obj.connect_external_port(name);
    }

    fn disconnect_external_port(&self, name: &str) {
        self.obj.disconnect_external_port(name);
    }
}

#[pyfunction]
pub fn open_driver_decoupled_midi_port<'py>
    (audio_driver : &AudioDriver,
     name_hint : &str,
     direction : i32) -> PyResult<DecoupledMidiPort> {
    let dir = backend_bindings::PortDirection::try_from(direction).unwrap();
    Ok(DecoupledMidiPort { obj: backend_bindings::DecoupledMidiPort::new_driver_port
                           (&audio_driver.obj,
                            name_hint,
                            &dir)
                                .unwrap() })
}

pub fn register_in_module<'py>(m: &Bound<'py, PyModule>) -> PyResult<()> {
    m.add_class::<DecoupledMidiPort>()?;
    m.add_function(wrap_pyfunction!(open_driver_decoupled_midi_port, m)?)?;
    Ok(())
}
