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
