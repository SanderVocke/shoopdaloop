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

    fn unsafe_backend_ptr (&self) -> usize {
        unsafe { self.obj.unsafe_backend_ptr() as usize }
    }
}

pub fn register_in_module<'py>(m: &Bound<'py, PyModule>) -> PyResult<()> {
    m.add_class::<FXChain>()?;
    Ok(())
}
