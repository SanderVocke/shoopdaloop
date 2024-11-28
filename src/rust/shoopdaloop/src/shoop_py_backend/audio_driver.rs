// See audio_driver.rs from backend_bindings

use pyo3::prelude::*;
// use pyo3::exceptions::*;
use backend_bindings;
use crate::shoop_py_backend::port::ExternalPortDescriptor;

#[pyclass]
pub struct AudioDriver {
    pub obj : backend_bindings::AudioDriver,
}

#[pymethods]
impl AudioDriver {
    #[new]
    fn py_new(driver_type : i32) -> PyResult<Self> {
        Ok(AudioDriver { obj: backend_bindings::AudioDriver::new
                                (backend_bindings::AudioDriverType::try_from(driver_type).unwrap()).unwrap() })
    }

    #[staticmethod]
    pub fn driver_type_supported(driver_type: i32) -> bool {
        backend_bindings::AudioDriver::driver_type_supported(
            backend_bindings::AudioDriverType::try_from(driver_type).unwrap(),
        )
    }

    pub fn start_dummy(&self, client_name: String, sample_rate: u32, buffer_size: u32) -> PyResult<()> {
        let settings = backend_bindings::DummyAudioDriverSettings {
            client_name,
            sample_rate,
            buffer_size,
        };
        self.obj.start_dummy(&settings).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    pub fn start_jack(&self, client_name_hint: String, maybe_server_name: Option<String>) -> PyResult<()> {
        let settings = backend_bindings::JackAudioDriverSettings {
            client_name_hint,
            maybe_server_name,
        };
        self.obj.start_jack(&settings).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    pub fn active(&self) -> bool {
        self.obj.active()
    }

    pub fn wait_process(&self) {
        self.obj.wait_process();
    }

    pub fn dummy_run_requested_frames(&self) {
        self.obj.dummy_run_requested_frames();
    }

    #[pyo3(signature = (maybe_name_regex=None, port_direction=0, data_type=0))]
    pub fn find_external_ports(&self, maybe_name_regex: Option<String>, port_direction: u32, data_type: u32) -> Vec<ExternalPortDescriptor> {
        self.obj
           .find_external_ports(maybe_name_regex.as_deref(), port_direction, data_type)
           .into_iter()
           .map(|p| ExternalPortDescriptor::new(p))
           .collect()
    }

    fn unsafe_backend_ptr (&self) -> usize {
        unsafe { self.obj.unsafe_backend_ptr() as usize }
    }
}

#[pyfunction]
pub fn driver_type_supported(driver_type : i32) -> bool {
    backend_bindings::AudioDriver::driver_type_supported(backend_bindings::AudioDriverType::try_from(driver_type).unwrap())
}

pub fn register_in_module<'py>(m: &Bound<'py, PyModule>) -> PyResult<()> {
    m.add_class::<AudioDriver>()?;
    m.add_function(wrap_pyfunction!(driver_type_supported, m)?)?;
    Ok(())
}
