// See audio_channel.rs from backend_bindings

use pyo3::prelude::*;
use backend_bindings;

#[pyclass]
pub struct AudioChannel {
    pub obj : backend_bindings::AudioChannel,
}

#[pymethods]
impl AudioChannel {
    fn unsafe_backend_ptr (&self) -> usize {
        unsafe { self.obj.unsafe_backend_ptr() as usize }
    }
}

pub fn register_in_module<'py>(m: &Bound<'py, PyModule>) -> PyResult<()> {
    m.add_class::<AudioChannel>()?;
    Ok(())
}