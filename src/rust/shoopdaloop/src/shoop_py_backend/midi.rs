use backend_bindings::MidiEvent as BackendMidiEvent;
use pyo3::prelude::*;

#[pyclass]
#[derive(Clone)]
pub struct MidiEvent {
    #[pyo3(get)]
    pub time: i32,
    #[pyo3(get)]
    pub data: Vec<u8>,
}

#[pymethods]
impl MidiEvent {
    #[new]
    fn new(time: i32, data: Vec<u8>) -> Self {
        MidiEvent { time, data }
    }
}

impl From<BackendMidiEvent> for MidiEvent {
    fn from(event: BackendMidiEvent) -> Self {
        MidiEvent {
            time: event.time,
            data: event.data,
        }
    }
}

impl MidiEvent {
    pub fn to_backend(&self) -> BackendMidiEvent {
        BackendMidiEvent {
            time: self.time,
            data: self.data.clone(),
        }
    }
}

pub fn register_in_module<'py>(m: &Bound<'py, PyModule>) -> PyResult<()> {
    m.add_class::<MidiEvent>()?;
    Ok(())
}
