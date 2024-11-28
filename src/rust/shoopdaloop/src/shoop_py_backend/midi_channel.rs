// See midi_channel.rs from backend_bindings

use pyo3::prelude::*;
use pyo3::exceptions::PyRuntimeError;
use backend_bindings::{MidiChannel as BackendMidiChannel, MidiChannelState as BackendMidiChannelState};
use crate::shoop_py_backend::midi::MidiEvent;
use crate::shoop_py_backend::channel::ChannelMode;

impl From<backend_bindings::MidiEvent> for MidiEvent {
    fn from(event: backend_bindings::MidiEvent) -> Self {
        MidiEvent {
            time: event.time,
            data: event.data,
        }
    }
}

#[pyclass]
#[derive(Clone)]
pub struct MidiChannelState {
    #[pyo3(get)]
    pub mode: ChannelMode,
    #[pyo3(get)]
    pub start_offset: i32,
    #[pyo3(get)]
    pub n_preplay_samples: u32,
    #[pyo3(get)]
    pub data_dirty: bool,
}

impl From<BackendMidiChannelState> for MidiChannelState {
    fn from(state: BackendMidiChannelState) -> Self {
        MidiChannelState {
            mode: state.mode,
            start_offset: state.start_offset,
            n_preplay_samples: state.n_preplay_samples,
            data_dirty: state.data_dirty,
        }
    }
}
use backend_bindings;

#[pyclass]
pub struct MidiChannel {
    pub obj : backend_bindings::MidiChannel,
}

#[pymethods]
impl MidiChannel {
    fn unsafe_backend_ptr (&self) -> usize {
        unsafe { self.obj.unsafe_backend_ptr() as usize }
    }

    fn get_all_midi_data(&self) -> PyResult<Vec<MidiEvent>> {
        let events = self.obj.get_all_midi_data();
        Ok(events.into_iter().map(MidiEvent::from).collect())
    }

    fn load_all_midi_data(&self, py: Python, msgs: Vec<MidiEvent>) -> PyResult<()> {
        let backend_msgs: Vec<BackendMidiEvent> = msgs.into_iter().map(|msg| BackendMidiEvent {
            time: msg.time,
            data: msg.data,
        }).collect();
        self.obj.load_all_midi_data(&backend_msgs);
        Ok(())
    }

    fn connect_input(&self, port: &MidiPort) -> PyResult<()> {
        self.obj.connect_input(&port.obj);
        Ok(())
    }

    fn connect_output(&self, port: &MidiPort) -> PyResult<()> {
        self.obj.connect_output(&port.obj);
        Ok(())
    }

    fn disconnect(&self, port: &MidiPort) -> PyResult<()> {
        self.obj.disconnect(&port.obj);
        Ok(())
    }

    fn get_state(&self) -> PyResult<MidiChannelState> {
        let state = self.obj.get_state().map_err(|e| {
            PyErr::new::<PyRuntimeError, _>(format!("Get state failed: {:?}", e))
        })?;
        Ok(MidiChannelState::from(state))
    }

    fn set_mode(&self, mode: i32) -> PyResult<()> {
        let mode = ChannelMode::try_from(mode).map_err(|_| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>("Invalid channel mode")
        })?;
        self.obj.set_mode(mode);
        Ok(())
    }

    fn set_start_offset(&self, offset: i32) -> PyResult<()> {
        self.obj.set_start_offset(offset);
        Ok(())
    }

    fn set_n_preplay_samples(&self, n: u32) -> PyResult<()> {
        self.obj.set_n_preplay_samples(n);
        Ok(())
    }

    fn clear_data_dirty(&self) -> PyResult<()> {
        self.obj.clear_data_dirty();
        Ok(())
    }

    fn clear(&self) -> PyResult<()> {
        self.obj.clear();
        Ok(())
    }

    fn reset_state_tracking(&self) -> PyResult<()> {
        self.obj.reset_state_tracking();
        Ok(())
    }
}

pub fn register_in_module<'py>(m: &Bound<'py, PyModule>) -> PyResult<()> {
    m.add_class::<MidiChannel>()?;
    m.add_class::<MidiChannelState>()?;
    Ok(())
}
