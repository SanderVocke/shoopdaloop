// See shoop_loop.rs from backend_bindings

use pyo3::prelude::*;
// use pyo3::exceptions::*;
use backend_bindings;

#[pyclass(eq, eq_int)]
#[derive(PartialEq, Clone)]
pub enum FXChainType {
    CarlaRack = backend_bindings::FXChainType::CarlaRack as isize,
    CarlaPatchbay = backend_bindings::FXChainType::CarlaPatchbay as isize,
    CarlaPatchbay16x = backend_bindings::FXChainType::CarlaPatchbay16x as isize,
    Test2x2x1 = backend_bindings::FXChainType::Test2x2x1 as isize,
}

impl TryFrom<backend_bindings::FXChainType> for FXChainType {
    type Error = anyhow::Error;
    fn try_from(value: backend_bindings::FXChainType) -> Result<Self, anyhow::Error> {
        match value {
            backend_bindings::FXChainType::CarlaRack => Ok(FXChainType::CarlaRack),
            backend_bindings::FXChainType::CarlaPatchbay => Ok(FXChainType::CarlaPatchbay),
            backend_bindings::FXChainType::CarlaPatchbay16x => Ok(FXChainType::CarlaPatchbay16x),
            backend_bindings::FXChainType::Test2x2x1 => Ok(FXChainType::Test2x2x1),
            _ => Err(anyhow::anyhow!("Invalid FXChainType")),
        }
    }
}

#[pymethods]
impl FXChainType {
    #[new]
    fn py_new(value: u32) -> PyResult<Self> {
        match backend_bindings::FXChainType::try_from(value) {
            Ok(val) => Ok(FXChainType::try_from(val).unwrap()),
            Err(_) => Err(PyErr::new::<pyo3::exceptions::PyValueError, _>("Invalid FXChainType")),
        }
    }
}

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

#[pyclass]
pub struct FXChain {
    pub obj : backend_bindings::FXChain,
}

#[pymethods]
impl FXChain {
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

    fn unsafe_backend_ptr (&self) -> usize { 
        unsafe { self.obj.unsafe_backend_ptr() as usize }
    }
}

pub fn register_in_module<'py>(m: &Bound<'py, PyModule>) -> PyResult<()> {
    m.add_class::<FXChain>()?;
    Ok(())
}
