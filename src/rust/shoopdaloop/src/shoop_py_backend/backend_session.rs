// See backend_session.rs from backend_bindings

use pyo3::prelude::*;
// use pyo3::exceptions::*;
use backend_bindings::{self, ProfilingReportItem as BackendProfilingReportItem};
use crate::shoop_py_backend::audio_driver::AudioDriver;
use backend_bindings::{ProfilingReport as BackendProfilingReport, ProfilingReportItem as BackendProfilingReportItem};

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
    pub fn from_backend(item: &BackendProfilingReportItem) -> Self {
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
    pub fn from_backend(report: &BackendProfilingReport) -> Self {
        ProfilingReport {
            items: report.items.iter().map(ProfilingReportItem::from_backend).collect(),
        }
    }
}

pub fn register_in_module<'py>(m: &PyModule) -> PyResult<()> {
    m.add_class::<ProfilingReport>()?;
    m.add_class::<ProfilingReportItem>()?;
    m.add_class::<BackendSession>()?;
    Ok(())
}
