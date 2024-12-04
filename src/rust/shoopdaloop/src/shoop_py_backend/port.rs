use pyo3::prelude::*;
use backend_bindings;

#[pyclass(eq, eq_int)]
#[derive(PartialEq, Clone)]
pub enum PortDirection {
    Input = backend_bindings::PortDirection::Input as isize,
    Output = backend_bindings::PortDirection::Output as isize,
    Any = backend_bindings::PortDirection::Any as isize,
}

#[pymethods]
impl PortDirection {
    #[staticmethod]
    pub fn enum_items() -> std::collections::HashMap<&'static str, isize> {
        let mut items = std::collections::HashMap::new();
        items.insert("Input", PortDirection::Input as isize);
        items.insert("Output", PortDirection::Output as isize);
        items.insert("Any", PortDirection::Any as isize);
        items
    }
}
    type Error = anyhow::Error;
    fn try_from(value: backend_bindings::PortDirection) -> Result<Self, anyhow::Error> {
        match value {
            backend_bindings::PortDirection::Input => Ok(PortDirection::Input),
            backend_bindings::PortDirection::Output => Ok(PortDirection::Output),
            backend_bindings::PortDirection::Any => Ok(PortDirection::Any),
        }
    }
}

#[pyclass(eq, eq_int)]
#[derive(PartialEq, Clone)]
pub enum PortDataType {
    Audio = backend_bindings::PortDataType::Audio as isize,
    Midi = backend_bindings::PortDataType::Midi as isize,
    Any = backend_bindings::PortDataType::Any as isize,
}

#[pymethods]
impl PortDataType {
    #[staticmethod]
    pub fn enum_items() -> std::collections::HashMap<&'static str, isize> {
        let mut items = std::collections::HashMap::new();
        items.insert("Audio", PortDataType::Audio as isize);
        items.insert("Midi", PortDataType::Midi as isize);
        items.insert("Any", PortDataType::Any as isize);
        items
    }
}
    type Error = anyhow::Error;
    fn try_from(value: backend_bindings::PortDataType) -> Result<Self, anyhow::Error> {
        match value {
            backend_bindings::PortDataType::Audio => Ok(PortDataType::Audio),
            backend_bindings::PortDataType::Midi => Ok(PortDataType::Midi),
            backend_bindings::PortDataType::Any => Ok(PortDataType::Any),
        }
    }
}

#[pyclass]
#[derive(Clone)]
pub struct ExternalPortDescriptor {
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub direction: PortDirection,
    #[pyo3(get)]
    pub data_type: PortDataType,
}

impl ExternalPortDescriptor {
    pub fn new(obj : backend_bindings::ExternalPortDescriptor) -> Self {
        return ExternalPortDescriptor {
            name : obj.name,
            direction : PortDirection::try_from(obj.direction).unwrap(),
            data_type : PortDataType::try_from(obj.data_type).unwrap(),
        }
    }
}

pub fn register_in_module<'py>(m: &Bound<'py, PyModule>) -> PyResult<()> {
    m.add_class::<PortDirection>()?;
    m.add_class::<PortDataType>()?;
    m.add_class::<ExternalPortDescriptor>()?;
    Ok(())
}
