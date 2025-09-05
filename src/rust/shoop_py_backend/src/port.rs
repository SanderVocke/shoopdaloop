use backend_bindings;
use pyo3::prelude::*;
use std::collections::HashMap;

#[pyclass(eq, eq_int)]
#[derive(PartialEq, Clone)]
pub enum PortDirection {
    Input = backend_bindings::PortDirection::Input as isize,
    Output = backend_bindings::PortDirection::Output as isize,
    Any = backend_bindings::PortDirection::Any as isize,
}

#[pymethods]
impl PortDirection {
    #[new]
    fn py_new(value: u32) -> PyResult<Self> {
        match backend_bindings::PortDirection::try_from(value as i32) {
            Ok(val) => Ok(PortDirection::try_from(val).unwrap()),
            Err(_) => Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Invalid PortDirection",
            )),
        }
    }

    #[staticmethod]
    pub fn enum_items() -> std::collections::HashMap<&'static str, isize> {
        let mut items = std::collections::HashMap::new();
        items.insert("Input", PortDirection::Input as isize);
        items.insert("Output", PortDirection::Output as isize);
        items.insert("Any", PortDirection::Any as isize);
        items
    }
}

impl TryFrom<backend_bindings::PortDirection> for PortDirection {
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
pub enum PortConnectabilityKind {
    None = backend_bindings::PortConnectabilityKind::None as isize,
    Internal = backend_bindings::PortConnectabilityKind::Internal as isize,
    External = backend_bindings::PortConnectabilityKind::External as isize,
}

#[pymethods]
impl PortConnectabilityKind {
    #[new]
    fn py_new(value: u32) -> PyResult<Self> {
        match backend_bindings::PortConnectabilityKind::try_from(value as i32) {
            Ok(val) => Ok(PortConnectabilityKind::try_from(val).unwrap()),
            Err(_) => Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Invalid PortConnectabilityKind",
            )),
        }
    }

    #[staticmethod]
    pub fn enum_items() -> HashMap<&'static str, isize> {
        let mut items = HashMap::new();
        items.insert("None", PortConnectabilityKind::None as isize);
        items.insert("Internal", PortConnectabilityKind::Internal as isize);
        items.insert("External", PortConnectabilityKind::External as isize);
        items
    }
}

impl TryFrom<backend_bindings::PortConnectabilityKind> for PortConnectabilityKind {
    type Error = anyhow::Error;
    fn try_from(value: backend_bindings::PortConnectabilityKind) -> Result<Self, anyhow::Error> {
        match value {
            backend_bindings::PortConnectabilityKind::None => Ok(PortConnectabilityKind::None),
            backend_bindings::PortConnectabilityKind::Internal => {
                Ok(PortConnectabilityKind::Internal)
            }
            backend_bindings::PortConnectabilityKind::External => {
                Ok(PortConnectabilityKind::External)
            }
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
    #[new]
    fn py_new(value: u32) -> PyResult<Self> {
        match backend_bindings::PortDataType::try_from(value as i32) {
            Ok(val) => Ok(PortDataType::try_from(val).unwrap()),
            Err(_) => Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Invalid PortDataType",
            )),
        }
    }

    #[staticmethod]
    pub fn enum_items() -> std::collections::HashMap<&'static str, isize> {
        let mut items = std::collections::HashMap::new();
        items.insert("Audio", PortDataType::Audio as isize);
        items.insert("Midi", PortDataType::Midi as isize);
        items.insert("Any", PortDataType::Any as isize);
        items
    }
}

impl TryFrom<backend_bindings::PortDataType> for PortDataType {
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
    pub fn new(obj: backend_bindings::ExternalPortDescriptor) -> Self {
        return ExternalPortDescriptor {
            name: obj.name,
            direction: PortDirection::try_from(obj.direction).unwrap(),
            data_type: PortDataType::try_from(obj.data_type).unwrap(),
        };
    }
}

#[pyclass]
#[derive(Clone)]
pub struct PortConnectability {
    #[pyo3(get)]
    pub internal: bool,
    #[pyo3(get)]
    pub external: bool,
}

#[pymethods]
impl PortConnectability {
    #[new]
    pub fn new(internal: bool, external: bool) -> Self {
        PortConnectability { internal, external }
    }
}

impl From<backend_bindings::PortConnectability> for PortConnectability {
    fn from(backend: backend_bindings::PortConnectability) -> Self {
        PortConnectability {
            internal: backend.internal,
            external: backend.external,
        }
    }
}

impl From<PortConnectability> for backend_bindings::PortConnectability {
    fn from(py: PortConnectability) -> Self {
        backend_bindings::PortConnectability {
            internal: py.internal,
            external: py.external,
        }
    }
}

pub fn register_in_module<'py>(m: &Bound<'py, PyModule>) -> PyResult<()> {
    m.add_class::<PortConnectabilityKind>()?;
    m.add_class::<PortConnectability>()?;
    m.add_class::<PortDirection>()?;
    m.add_class::<PortDataType>()?;
    m.add_class::<ExternalPortDescriptor>()?;
    Ok(())
}
