#[repr(C)]
pub struct AudioDriverState {
    pub dsp_load_percent: f32,
    pub xruns_since_last: u32,
    pub maybe_driver_handle: *mut std::os::raw::c_void,
    pub maybe_instance_name: *const std::os::raw::c_char,
    pub sample_rate: u32,
    pub buffer_size: u32,
    pub active: u32,
    pub last_processed: u32,
}

use pyo3::prelude::*;
// use pyo3::exceptions::*;
use backend_bindings;
use crate::shoop_py_backend::port;

#[pyclass]
pub struct JackAudioDriverSettings {
    pub client_name_hint: String,
    pub maybe_server_name: Option<String>,
}

impl JackAudioDriverSettings {
    fn to_ffi(&self) -> backend_bindings::JackAudioDriverSettings {
        backend_bindings::JackAudioDriverSettings {
            client_name_hint: self.client_name_hint.clone(),
            maybe_server_name: self.maybe_server_name.clone(),
        }
    }
}

#[pymethods]
impl JackAudioDriverSettings {
    #[new]
    fn py_new(client_name_hint: String, maybe_server_name: Option<String>) -> Self {
        JackAudioDriverSettings { client_name_hint, maybe_server_name }
    }
}

#[pyclass]
pub struct DummyAudioDriverSettings {
    pub client_name: String,
    pub sample_rate: u32,
    pub buffer_size: u32,
}

impl DummyAudioDriverSettings {
    fn to_ffi(&self) -> backend_bindings::DummyAudioDriverSettings {
        backend_bindings::DummyAudioDriverSettings {
            client_name: self.client_name.clone(),
            sample_rate: self.sample_rate,
            buffer_size: self.buffer_size,
        }
    }
}

#[pymethods]
impl DummyAudioDriverSettings {
    #[new]
    fn py_new(client_name: String, sample_rate: u32, buffer_size: u32) -> Self {
        DummyAudioDriverSettings { client_name, sample_rate, buffer_size }
    }
}

#[pyclass]
pub struct AudioDriver {
    pub obj: backend_bindings::AudioDriver,
}

#[pymethods]
impl AudioDriver {
    #[new]
    fn py_new(driver_type: u32) -> PyResult<Self> {
        let driver_type = backend_bindings::AudioDriverType::try_from(driver_type).map_err(|_| PyErr::new::<pyo3::exceptions::PyValueError, _>("Invalid driver type"))?;
        let obj = backend_bindings::AudioDriver::new(driver_type).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Failed to create AudioDriver: {:?}", e)))?;
        Ok(AudioDriver { obj })
    }

    fn get_sample_rate(&self) -> u32 {
        self.obj.get_sample_rate()
    }

    fn get_buffer_size(&self) -> u32 {
        self.obj.get_buffer_size()
    }

    fn start_dummy(&self, settings: &DummyAudioDriverSettings) -> PyResult<()> {
        self.obj.start_dummy(&settings.to_ffi()).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Failed to start dummy driver: {:?}", e)))
    }

    fn start_jack(&self, settings: &JackAudioDriverSettings) -> PyResult<()> {
        self.obj.start_jack(&settings.to_ffi()).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Failed to start jack driver: {:?}", e)))
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

    #[pyo3(signature = (maybe_name_regex=None, port_direction=0, data_type=0))]
    fn find_external_ports(&self, maybe_name_regex: Option<&str>, port_direction: u32, data_type: u32) -> Vec<port::ExternalPortDescriptor> {
        self.obj.find_external_ports(maybe_name_regex, port_direction, data_type)
            .into_iter()
            .map(port::ExternalPortDescriptor::new)
            .collect()
    }
}

pub fn register_in_module<'py>(m: &Bound<'py, PyModule>) -> PyResult<()> {
    m.add_class::<AudioDriver>()?;
    m.add_class::<JackAudioDriverSettings>()?;
    m.add_class::<DummyAudioDriverSettings>()?;
    Ok(())
}
