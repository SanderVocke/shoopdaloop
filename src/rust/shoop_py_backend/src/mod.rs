use pyo3::prelude::*;

pub mod audio_channel;
pub mod audio_driver;
pub mod audio_port;
pub mod backend_session;
pub mod channel;
pub mod decoupled_midi_port;
pub mod fx_chain;
pub mod logging;
pub mod midi;
pub mod midi_channel;
pub mod midi_port;
pub mod port;
pub mod resample;
pub mod shoop_loop;

pub fn create_py_module<'py>(py: Python<'py>) -> Result<Bound<'py, PyModule>, PyErr> {
    let m = PyModule::new_bound(py, "shoop_py_backend")?;
    resample::register_in_module(&m)?;
    audio_channel::register_in_module(&m)?;
    audio_driver::register_in_module(&m)?;
    audio_port::register_in_module(&m)?;
    backend_session::register_in_module(&m)?;
    channel::register_in_module(&m)?;
    decoupled_midi_port::register_in_module(&m)?;
    fx_chain::register_in_module(&m)?;
    logging::register_in_module(&m)?;
    midi::register_in_module(&m)?;
    midi_channel::register_in_module(&m)?;
    midi_port::register_in_module(&m)?;
    port::register_in_module(&m)?;
    shoop_loop::register_in_module(&m)?;
    Ok(m)
}
