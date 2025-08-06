use pyo3::prelude::*;
// use pyo3::exceptions::*;
use crate::port;
use backend_bindings;
use std::collections::HashMap;

#[pyclass(eq, eq_int)]
#[derive(PartialEq, Clone)]
pub enum AudioDriverType {
    Jack = backend_bindings::AudioDriverType::Jack as isize,
    JackTest = backend_bindings::AudioDriverType::JackTest as isize,
    Dummy = backend_bindings::AudioDriverType::Dummy as isize,
}

#[pymethods]
impl AudioDriverType {
    #[new]
    fn py_new(value: u32) -> PyResult<Self> {
        match backend_bindings::AudioDriverType::try_from(value) {
            Ok(val) => Ok(AudioDriverType::try_from(val).unwrap()),
            Err(_) => Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Invalid AudioDriverType",
            )),
        }
    }

    #[staticmethod]
    pub fn enum_items() -> HashMap<&'static str, isize> {
        let mut items = HashMap::new();
        items.insert("Jack", AudioDriverType::Jack as isize);
        items.insert("JackTest", AudioDriverType::JackTest as isize);
        items.insert("Dummy", AudioDriverType::Dummy as isize);
        items
    }

    pub fn name(&self) -> &'static str {
        match self {
            AudioDriverType::Jack => "Jack",
            AudioDriverType::JackTest => "JackTest",
            AudioDriverType::Dummy => "Dummy",
        }
    }
}

impl TryFrom<backend_bindings::AudioDriverType> for AudioDriverType {
    type Error = anyhow::Error;
    fn try_from(value: backend_bindings::AudioDriverType) -> Result<Self, anyhow::Error> {
        match value {
            backend_bindings::AudioDriverType::Jack => Ok(AudioDriverType::Jack),
            backend_bindings::AudioDriverType::JackTest => Ok(AudioDriverType::JackTest),
            backend_bindings::AudioDriverType::Dummy => Ok(AudioDriverType::Dummy),
        }
    }
}

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
    #[pyo3(signature = (client_name_hint, maybe_server_name=None))]
    fn py_new(client_name_hint: String, maybe_server_name: Option<String>) -> Self {
        JackAudioDriverSettings {
            client_name_hint,
            maybe_server_name,
        }
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
        DummyAudioDriverSettings {
            client_name,
            sample_rate,
            buffer_size,
        }
    }
}

#[pyclass]
pub struct AudioDriverState {
    #[pyo3(get)]
    pub dsp_load_percent: f32,
    #[pyo3(get)]
    pub xruns_since_last: u32,
    #[pyo3(get)]
    pub maybe_instance_name: String,
    #[pyo3(get)]
    pub sample_rate: u32,
    #[pyo3(get)]
    pub buffer_size: u32,
    #[pyo3(get)]
    pub active: u32,
    #[pyo3(get)]
    pub last_processed: u32,
}

impl AudioDriverState {
    pub fn new(obj: &backend_bindings::AudioDriverState) -> Self {
        return AudioDriverState {
            dsp_load_percent: obj.dsp_load_percent,
            xruns_since_last: obj.xruns_since_last,
            maybe_instance_name: obj.maybe_instance_name.clone(),
            sample_rate: obj.sample_rate,
            buffer_size: obj.buffer_size,
            active: obj.active,
            last_processed: obj.last_processed,
        };
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
        let driver_type = backend_bindings::AudioDriverType::try_from(driver_type)
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyValueError, _>("Invalid driver type"))?;
        let obj = backend_bindings::AudioDriver::new(driver_type, None).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "Failed to create AudioDriver: {:?}",
                e
            ))
        })?;
        Ok(AudioDriver { obj })
    }

    fn get_sample_rate(&self) -> u32 {
        self.obj.get_sample_rate()
    }

    fn get_buffer_size(&self) -> u32 {
        self.obj.get_buffer_size()
    }

    fn start_dummy(&self, settings: &DummyAudioDriverSettings) -> PyResult<()> {
        self.obj.start_dummy(&settings.to_ffi()).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "Failed to start dummy driver: {:?}",
                e
            ))
        })
    }

    fn start_jack(&self, settings: &JackAudioDriverSettings) -> PyResult<()> {
        self.obj.start_jack(&settings.to_ffi()).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "Failed to start jack driver: {:?}",
                e
            ))
        })
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
        self.obj
            .dummy_add_external_mock_port(name, direction, data_type)
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
    fn find_external_ports(
        &self,
        maybe_name_regex: Option<&str>,
        port_direction: u32,
        data_type: u32,
    ) -> Vec<port::ExternalPortDescriptor> {
        self.obj
            .find_external_ports(maybe_name_regex, port_direction, data_type)
            .into_iter()
            .map(port::ExternalPortDescriptor::new)
            .collect()
    }

    fn get_state(&self) -> AudioDriverState {
        AudioDriverState::new(&self.obj.get_state())
    }
}

#[pyfunction]
pub fn driver_type_supported(driver_type: u32) -> bool {
    backend_bindings::driver_type_supported(
        backend_bindings::AudioDriverType::try_from(driver_type).unwrap(),
    )
}

pub fn register_in_module<'py>(m: &Bound<'py, PyModule>) -> PyResult<()> {
    m.add_class::<AudioDriver>()?;
    m.add_class::<AudioDriverType>()?;
    m.add_class::<JackAudioDriverSettings>()?;
    m.add_class::<DummyAudioDriverSettings>()?;
    m.add_function(wrap_pyfunction!(driver_type_supported, m)?)?;
    Ok(())
}
