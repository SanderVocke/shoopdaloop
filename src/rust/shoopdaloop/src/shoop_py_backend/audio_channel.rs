// See audio_channel.rs from backend_bindings

use pyo3::prelude::*;
use backend_bindings;

use crate::shoop_py_backend::audio_port::AudioPort;
use crate::shoop_py_backend::channel::ChannelMode;

#[pyclass]
pub struct AudioChannelState {
    #[pyo3(get)]
    pub mode : ChannelMode,
    #[pyo3(get)]
    pub gain : f32,
    #[pyo3(get)]
    pub output_peak : f32,
    #[pyo3(get)]
    pub length : u32,
    #[pyo3(get)]
    pub start_offset : i32,
    #[pyo3(get)]
    pub played_back_sample : Option<u32>,
    #[pyo3(get)]
    pub n_preplay_samples : u32,
    #[pyo3(get)]
    pub data_dirty : bool,
}

impl AudioChannelState {
    pub fn new(obj : backend_bindings::AudioChannelState) -> Self {
        return AudioChannelState {
            mode : ChannelMode::try_from(obj.mode).unwrap(),
            gain : obj.gain,
            output_peak : obj.output_peak,
            length : obj.length,
            start_offset : obj.start_offset,
            played_back_sample : obj.played_back_sample,
            n_preplay_samples : obj.n_preplay_samples,
            data_dirty : obj.data_dirty,
        }
    }
}


#[pyclass]
pub struct AudioChannel {
    pub obj : backend_bindings::AudioChannel,
}

#[pymethods]
impl AudioChannel {
    fn unsafe_backend_ptr (&self) -> usize {
        unsafe { self.obj.unsafe_backend_ptr() as usize }
    }

    fn connect_input(&self, port: &AudioPort) -> PyResult<()> {
        self.obj.connect_input(&port.obj);
        Ok(())
    }

    fn connect_output(&self, port: &AudioPort) -> PyResult<()> {
        self.obj.connect_output(&port.obj);
        Ok(())
    }

    fn disconnect(&self, port: &AudioPort) -> PyResult<()> {
        self.obj.disconnect(&port.obj);
        Ok(())
    }

    fn load_data(&self, data: Vec<f32>) -> PyResult<()> {
        self.obj.load_data(&data);
        Ok(())
    }

    fn get_data(&self) -> PyResult<Vec<f32>> {
        Ok(self.obj.get_data())
    }

    fn get_state(&self) -> PyResult<AudioChannelState> {
        let state = self.obj.get_state().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Get state failed: {:?}", e))
        })?;
        Ok(AudioChannelState::new(state))
    }

    fn set_gain(&self, gain: f32) -> PyResult<()> {
        self.obj.set_gain(gain);
        Ok(())
    }

    fn set_mode(&self, mode: i32) -> PyResult<()> {
        let mode = backend_bindings::ChannelMode::try_from(mode).map_err(|_| {
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

    fn clear(&self, length: u32) -> PyResult<()> {
        self.obj.clear(length);
        Ok(())
    }
}

pub fn register_in_module<'py>(m: &Bound<'py, PyModule>) -> PyResult<()> {
    m.add_class::<AudioChannel>()?;
    m.add_class::<AudioChannelState>()?;
    Ok(())
}
