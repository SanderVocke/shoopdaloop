// See midi_channel.rs from backend_bindings

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use crate::shoop_py_backend::midi_port::MidiPort;

use crate::shoop_py_backend::channel::ChannelMode;
use crate::shoop_py_backend::midi::MidiEvent;

#[pyclass]
#[derive(Clone)]
pub struct MidiChannelState {
    #[pyo3(get)]
    pub mode: ChannelMode,
    #[pyo3(get)]
    pub n_events_triggered: u32,
    #[pyo3(get)]
    pub n_notes_active: u32,
    #[pyo3(get)]
    pub length: u32,
    #[pyo3(get)]
    pub start_offset: i32,
    #[pyo3(get)]
    pub played_back_sample: Option<u32>,
    #[pyo3(get)]
    pub n_preplay_samples: u32,
    #[pyo3(get)]
    pub data_dirty: bool,
}

impl MidiChannelState {
    fn new(state: backend_bindings::MidiChannelState) -> Self {
        MidiChannelState {
            mode: ChannelMode::try_from(state.mode).unwrap(),
            n_events_triggered: state.n_events_triggered,
            n_notes_active: state.n_notes_active,
            length: state.length,
            start_offset: state.start_offset,
            played_back_sample: state.played_back_sample,
            n_preplay_samples: state.n_preplay_samples,
            data_dirty: state.data_dirty,
        }
    }
}
use backend_bindings;

#[pyclass]
pub struct MidiChannel {
    pub obj: backend_bindings::MidiChannel,
}

#[pymethods]
impl MidiChannel {
    fn unsafe_backend_ptr(&self) -> usize {
        unsafe { self.obj.unsafe_backend_ptr() as usize }
    }

    fn get_all_midi_data(&self) -> PyResult<Vec<MidiEvent>> {
        let events = self.obj.get_all_midi_data();
        Ok(events.into_iter().map(|e| MidiEvent::from(e)).collect())
    }

    fn load_all_midi_data(&self, _py: Python, msgs: Vec<MidiEvent>) -> PyResult<()> {
        let backend_msgs: Vec<backend_bindings::MidiEvent> =
            msgs.into_iter().map(|msg| msg.to_backend()).collect();
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
        let state = self
            .obj
            .get_state()
            .map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("Get state failed: {:?}", e)))?;
        Ok(MidiChannelState::new(state))
    }

    fn set_mode(&self, mode: i32) -> PyResult<()> {
        let mode = backend_bindings::ChannelMode::try_from(mode)
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyValueError, _>("Invalid channel mode"))?;
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

    fn clear(&self, _length: u32) -> PyResult<()> {
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
