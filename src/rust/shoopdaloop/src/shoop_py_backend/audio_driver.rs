// See audio_driver.rs from backend_bindings

use pyo3::prelude::*;
// use pyo3::exceptions::*;
use backend_bindings;

#[pyclass]
pub struct AudioDriver {
    obj : backend_bindings::AudioDriver,
}

#[pymethods]
impl AudioDriver {
    #[new]
    fn py_new(driver_type : i32) -> PyResult<Self> {
        Ok(AudioDriver { obj: backend_bindings::AudioDriver::new
                                (backend_bindings::AudioDriverType::try_from(driver_type).unwrap()).unwrap() })
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