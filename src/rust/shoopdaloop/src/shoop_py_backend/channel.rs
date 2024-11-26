use backend_bindings;
use pyo3::prelude::*;

#[pyclass(eq, eq_int)]
#[derive(PartialEq, Clone)]
pub enum ChannelMode {
    Disabled = backend_bindings::ChannelMode::Disabled as isize,
    Direct = backend_bindings::ChannelMode::Direct as isize,
    Dry = backend_bindings::ChannelMode::Dry as isize,
    Wet = backend_bindings::ChannelMode::Wet as isize,
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