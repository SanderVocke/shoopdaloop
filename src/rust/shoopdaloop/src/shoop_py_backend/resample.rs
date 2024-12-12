// See resample.rs from backend_bindings

use pyo3::prelude::*;
// use pyo3::exceptions::*;
use backend_bindings;

#[pyclass]
pub struct MultichannelAudio {
    obj : backend_bindings::MultichannelAudio,
}

#[pymethods]
impl MultichannelAudio {
    #[new]
    fn py_new(n_channels : u32, n_frames : u32) -> PyResult<Self> {
        Ok(MultichannelAudio { obj: backend_bindings::MultichannelAudio::new(n_channels as u32, n_frames as u32).unwrap() })
    }

    fn resample(&self, new_n_frames : u32) -> PyResult<MultichannelAudio> {
        match self.obj.resample(new_n_frames) {
            Ok(new_obj) => Ok(MultichannelAudio { obj: new_obj }),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyException, _>(format!("{}", e))),
        }
    }

    fn at(&self, frame : u32, channel: u32) -> PyResult<f32> {
        match self.obj.at(frame, channel) {
            Ok(value) => Ok(value),
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyException, _>(format!("{}", e))),
        }
    }
}

pub fn register_in_module<'py>(m: &Bound<'py, PyModule>) -> PyResult<()> {
    m.add_class::<MultichannelAudio>()?;
    Ok(())
}