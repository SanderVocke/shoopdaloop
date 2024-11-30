use pyo3::prelude::*;

mod audio_channel;
mod audio_driver;
mod audio_port;
mod backend_session;
mod channel;
mod decoupled_midi_port;
mod fx_chain;
mod midi;
mod midi_channel;
mod midi_port;
mod port;
mod resample;
mod shoop_loop;

pub fn create_py_module<'py>(
    py : Python<'py>
) -> Result<Bound<'py, PyModule>, PyErr> {
    let m = PyModule::new_bound(py, "shoop_py_backend")?;
    resample::register_in_module(&m)?;
    audio_channel::register_in_module(&m)?;
    audio_driver::register_in_module(&m)?;
    audio_port::register_in_module(&m)?;
    backend_session::register_in_module(&m)?;
    channel::register_in_module(&m)?;
    decoupled_midi_port::register_in_module(&m)?;
    fx_chain::register_in_module(&m)?;
    midi::register_in_module(&m)?;
    midi_channel::register_in_module(&m)?;
    midi_port::register_in_module(&m)?;
    port::register_in_module(&m)?;
    shoop_loop::register_in_module(&m)?;
    Ok(m)
}
