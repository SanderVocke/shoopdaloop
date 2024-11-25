// See audio_port.rs from backend_bindings

use pyo3::prelude::*;
// use pyo3::exceptions::*;
use backend_bindings;

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