use pyo3::prelude::*;

mod audio_channel;
mod audio_driver;
mod backend_session;
mod midi_channel;
mod resample;
mod shoop_loop;

pub fn create_py_module<'py>(
    py : Python<'py>
) -> Result<Bound<'py, PyModule>, PyErr> {
    let m = PyModule::new_bound(py, "shoop_py_backend")?;
    resample::register_in_module(&m)?;
    audio_channel::register_in_module(&m)?;
    audio_driver::register_in_module(&m)?;
    backend_session::register_in_module(&m)?;
    midi_channel::register_in_module(&m)?;
    shoop_loop::register_in_module(&m)?;
    Ok(m)
}