// See shoop_loop.rs from backend_bindings

use pyo3::prelude::*;
// use pyo3::exceptions::*;
use backend_bindings;

use crate::shoop_py_backend::backend_session::BackendSession;
use crate::shoop_py_backend::audio_channel::AudioChannel;
use crate::shoop_py_backend::midi_channel::MidiChannel;

#[pyclass]
pub struct Loop {
    pub obj : backend_bindings::Loop,
}

#[pymethods]
impl Loop {
    #[new]
    fn py_new(backend_session : &BackendSession) -> PyResult<Self> {
        Ok(Loop { obj: backend_bindings::Loop::new(&backend_session.obj).unwrap() })
    }

    fn add_audio_channel(&self, mode : i32) -> PyResult<AudioChannel> {
        match backend_bindings::ChannelMode::try_from(mode) {
            Ok(value) => Ok(AudioChannel { obj: self.obj.add_audio_channel(value).unwrap() }),
            Err(_) => {
                Err(PyErr::new::<pyo3::exceptions::PyValueError, _>("Invalid channel mode"))
            }
        }
    }

    fn add_midi_channel(&self, mode : i32) -> PyResult<MidiChannel> {
        match backend_bindings::ChannelMode::try_from(mode) {
            Ok(value) => Ok(MidiChannel { obj: self.obj.add_midi_channel(value).unwrap() }),
            Err(_) => {
                Err(PyErr::new::<pyo3::exceptions::PyValueError, _>("Invalid channel mode"))
            }
        }
    }

    fn unsafe_backend_ptr (&self) -> usize {
        unsafe { self.obj.unsafe_backend_ptr() as usize }
    }
}

pub fn register_in_module<'py>(m: &Bound<'py, PyModule>) -> PyResult<()> {
    m.add_class::<Loop>()?;
    Ok(())
}