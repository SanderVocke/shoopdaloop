// See shoop_loop.rs from backend_bindings

use pyo3::prelude::*;
// use pyo3::exceptions::*;
use backend_bindings;

use crate::shoop_py_backend::backend_session::BackendSession;

#[pyclass]
pub struct FXChainState {
    #[pyo3(get)]
    pub ready: u32,
    #[pyo3(get)]
    pub active: u32,
    #[pyo3(get)]
    pub visible: u32,
}

impl FXChainState {
    pub fn new(obj: backend_bindings::FXChainState) -> Self {
        FXChainState {
            ready: obj.ready,
            active: obj.active,
            visible: obj.visible,
        }
    }
}
pub struct FXChain {
    pub obj : backend_bindings::FXChain,
}

#[pymethods]
impl FXChain {
    #[new]
    fn py_new(backend_session : &BackendSession,
              chain_type : u32,
              title : &str) -> PyResult<Self> {
        Ok(FXChain { obj: backend_bindings::FXChain::new
                             (&backend_session.obj,
                              &backend_bindings::FXChainType::try_from(chain_type).unwrap(),
                              title).unwrap() })
    }

    fn available(&self) -> bool {
        self.obj.available()
    }

    fn set_visible(&self, visible: bool) {
        self.obj.set_visible(visible);
    }

    fn set_active(&self, active: bool) {
        self.obj.set_active(active);
    }

    fn get_state(&self) -> PyResult<FXChainState> {
        match self.obj.get_state() {
            Some(state) => Ok(FXChainState::new(state)),
            None => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Failed to get state")),
        }
    }

    fn get_state_str(&self) -> Option<String> {
        self.obj.get_state_str()
    }

    fn get_audio_input_port(&self, idx: u32) -> Option<usize> {
        self.obj.get_audio_input_port(idx).map(|ptr| ptr as usize)
    }

    fn get_audio_output_port(&self, idx: u32) -> Option<usize> {
        self.obj.get_audio_output_port(idx).map(|ptr| ptr as usize)
    }

    fn get_midi_input_port(&self, idx: u32) -> Option<usize> {
        self.obj.get_midi_input_port(idx).map(|ptr| ptr as usize)
    }

    fn restore_state(&self, state_str: &str) {
        self.obj.restore_state(state_str);
    }
        unsafe { self.obj.unsafe_backend_ptr() as usize }
    }
}

pub fn register_in_module<'py>(m: &Bound<'py, PyModule>) -> PyResult<()> {
    m.add_class::<FXChain>()?;
    Ok(())
}
