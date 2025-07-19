use backend_bindings;
use pyo3::prelude::*;

use std::collections::HashMap;

#[pyclass(eq, eq_int)]
#[derive(PartialEq, Clone)]
pub enum ChannelMode {
    Disabled = backend_bindings::ChannelMode::Disabled as isize,
    Direct = backend_bindings::ChannelMode::Direct as isize,
    Dry = backend_bindings::ChannelMode::Dry as isize,
    Wet = backend_bindings::ChannelMode::Wet as isize,
}

#[pymethods]
impl ChannelMode {
    #[new]
    fn py_new(value: u32) -> PyResult<Self> {
        match backend_bindings::ChannelMode::try_from(value) {
            Ok(val) => Ok(ChannelMode::try_from(val).unwrap()),
            Err(_) => Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Invalid ChannelMode",
            )),
        }
    }

    #[staticmethod]
    pub fn enum_items() -> HashMap<&'static str, isize> {
        let mut items = HashMap::new();
        items.insert("Disabled", ChannelMode::Disabled as isize);
        items.insert("Direct", ChannelMode::Direct as isize);
        items.insert("Dry", ChannelMode::Dry as isize);
        items.insert("Wet", ChannelMode::Wet as isize);
        items
    }
}

impl TryFrom<backend_bindings::ChannelMode> for ChannelMode {
    type Error = anyhow::Error;
    fn try_from(value: backend_bindings::ChannelMode) -> Result<Self, anyhow::Error> {
        match value {
            backend_bindings::ChannelMode::Disabled => Ok(ChannelMode::Disabled),
            backend_bindings::ChannelMode::Direct => Ok(ChannelMode::Direct),
            backend_bindings::ChannelMode::Dry => Ok(ChannelMode::Dry),
            backend_bindings::ChannelMode::Wet => Ok(ChannelMode::Wet),
        }
    }
}

pub fn register_in_module<'py>(m: &Bound<'py, PyModule>) -> PyResult<()> {
    m.add_class::<ChannelMode>()?;
    Ok(())
}
