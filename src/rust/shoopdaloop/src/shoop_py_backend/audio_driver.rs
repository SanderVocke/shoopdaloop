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


    pub fn unsafe_backend_ptr(&self) -> usize {
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
use pyo3::prelude::*;
use backend_bindings::{AudioDriver as RustAudioDriver, AudioDriverType, DummyAudioDriverSettings, JackAudioDriverSettings, ExternalPortDescriptor};

#[pyclass]
pub struct AudioDriver {
    pub obj: RustAudioDriver,
}

#[pymethods]
impl AudioDriver {
    #[new]
    fn py_new(driver_type: u32) -> PyResult<Self> {
        let driver_type = AudioDriverType::try_from(driver_type).map_err(|_| PyErr::new::<pyo3::exceptions::PyValueError, _>("Invalid driver type"))?;
        let obj = RustAudioDriver::new(driver_type).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Failed to create AudioDriver: {:?}", e)))?;
        Ok(AudioDriver { obj })
    }

    fn get_sample_rate(&self) -> u32 {
        self.obj.get_sample_rate()
    }

    fn get_buffer_size(&self) -> u32 {
        self.obj.get_buffer_size()
    }

    fn start_dummy(&self, settings: &DummyAudioDriverSettings) -> PyResult<()> {
        self.obj.start_dummy(settings).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Failed to start dummy driver: {:?}", e)))
    }

    fn start_jack(&self, settings: &JackAudioDriverSettings) -> PyResult<()> {
        self.obj.start_jack(settings).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Failed to start jack driver: {:?}", e)))
    }

    fn active(&self) -> bool {
        self.obj.active()
    }

    fn wait_process(&self) {
        self.obj.wait_process()
    }

    fn dummy_enter_controlled_mode(&self) {
        self.obj.dummy_enter_controlled_mode()
    }

    fn dummy_enter_automatic_mode(&self) {
        self.obj.dummy_enter_automatic_mode()
    }

    fn dummy_is_controlled(&self) -> bool {
        self.obj.dummy_is_controlled()
    }

    fn dummy_request_controlled_frames(&self, n: u32) {
        self.obj.dummy_request_controlled_frames(n)
    }

    fn dummy_n_requested_frames(&self) -> u32 {
        self.obj.dummy_n_requested_frames()
    }

    fn dummy_add_external_mock_port(&self, name: &str, direction: u32, data_type: u32) {
        self.obj.dummy_add_external_mock_port(name, direction, data_type)
    }

    fn dummy_remove_external_mock_port(&self, name: &str) {
        self.obj.dummy_remove_external_mock_port(name)
    }

    fn dummy_remove_all_external_mock_ports(&self) {
        self.obj.dummy_remove_all_external_mock_ports()
    }

    fn dummy_run_requested_frames(&self) {
        self.obj.dummy_run_requested_frames()
    }

    fn find_external_ports(&self, maybe_name_regex: Option<&str>, port_direction: u32, data_type: u32) -> Vec<ExternalPortDescriptor> {
        self.obj.find_external_ports(maybe_name_regex, port_direction, data_type)
    }
}
