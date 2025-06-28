// See backend_session.rs from backend_bindings

use pyo3::prelude::*;
// use pyo3::exceptions::*;
use backend_bindings;
use crate::shoop_py_backend::audio_driver::AudioDriver;
use crate::shoop_py_backend::shoop_loop::Loop;
use crate::shoop_py_backend::fx_chain::FXChain;

#[pyclass]
pub struct BackendSessionState {
    #[pyo3(get)]
    pub audio_driver: usize,
}

impl BackendSessionState {
    pub fn from_backend(state: &backend_bindings::BackendSessionState) -> Self {
        BackendSessionState {
            audio_driver: state.audio_driver as usize,
        }
    }
}

#[pyclass]
pub struct BackendSession {
    pub obj : backend_bindings::BackendSession,
}

#[pymethods]
impl BackendSession {
    #[new]
    fn py_new() -> PyResult<Self> {
        Ok(BackendSession { obj: backend_bindings::BackendSession::new().unwrap() })
    }

    fn unsafe_backend_ptr (&self) -> usize {
        unsafe { self.obj.unsafe_backend_ptr() as usize }
    }

    fn set_audio_driver(&self, driver : &AudioDriver) -> PyResult<()> {
        if self.obj.set_audio_driver(&driver.obj).is_ok() {
            Ok(())
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyException, _>("set_audio_driver() failed"))
        }
    }

    fn create_loop(&self) -> PyResult<Loop> {
        let obj = self.obj.create_loop();
        if obj.is_err() {
            return Err(PyErr::new::<pyo3::exceptions::PyException, _>("Failed to create loop"));
        }
        let obj = Loop::new(obj.unwrap());
        Ok(obj)
    }

    fn create_fx_chain(&self, chain_type: u32, title: &str) -> PyResult<FXChain> {
        let obj = self.obj.create_fx_chain(chain_type.try_into().unwrap(), title);
        if obj.is_err() {
            return Err(PyErr::new::<pyo3::exceptions::PyException, _>("Failed to create fx chain"));
        }
        let obj = obj.unwrap();
        let wrapped = FXChain { obj };
        Ok(wrapped)
    }

    fn get_profiling_report(&self) -> PyResult<ProfilingReport> {
        let backend_obj = self.obj.get_profiling_report();
        Ok(ProfilingReport::from_backend(&backend_obj))
    }
}

#[pyclass]
pub struct ProfilingReportItem {
    #[pyo3(get)]
    pub key: String,
    #[pyo3(get)]
    pub n_samples: f32,
    #[pyo3(get)]
    pub average: f32,
    #[pyo3(get)]
    pub worst: f32,
    #[pyo3(get)]
    pub most_recent: f32,
}

impl ProfilingReportItem {
    pub fn from_backend(item: &backend_bindings::ProfilingReportItem) -> Self {
        ProfilingReportItem {
            key: item.key.clone(),
            n_samples: item.n_samples,
            average: item.average,
            worst: item.worst,
            most_recent: item.most_recent,
        }
    }
}

#[pyclass]
pub struct ProfilingReport {
    #[pyo3(get)]
    pub items: Vec<ProfilingReportItem>,
}

impl ProfilingReport {
    pub fn from_backend(report: &backend_bindings::ProfilingReport) -> Self {
        ProfilingReport {
            items: report.items.iter().map(ProfilingReportItem::from_backend).collect(),
        }
    }
}

impl Clone for ProfilingReportItem {
    fn clone(&self) -> Self {
        ProfilingReportItem {
            key: self.key.clone(),
            n_samples: self.n_samples,
            average: self.average,
            worst: self.worst,
            most_recent: self.most_recent,
        }
    }
}

pub fn register_in_module<'py>(m: &Bound<'py, PyModule>) -> PyResult<()> {
    m.add_class::<ProfilingReport>()?;
    m.add_class::<ProfilingReportItem>()?;
    m.add_class::<BackendSessionState>()?;
    m.add_class::<BackendSession>()?;
    Ok(())
}
