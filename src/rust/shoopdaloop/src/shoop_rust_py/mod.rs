use pyo3::prelude::*;
mod init;
mod objects;

pub fn create_py_module<'py>(
    py : Python<'py>
) -> Result<Bound<'py, PyModule>, PyErr> {
    let m = PyModule::new_bound(py, "shoop_rust")?;
    m.add_function(wrap_pyfunction!(init::shoop_rust_init, &m)?)?;
    m.add_function(wrap_pyfunction!(objects::shoop_rust_create_autoconnect, &m)?)?;
    m.add_function(wrap_pyfunction!(objects::shoop_rust_create_loop, &m)?)?;
    m.add_function(wrap_pyfunction!(objects::shoop_rust_open_driver_audio_port, &m)?)?;
    m.add_function(wrap_pyfunction!(objects::shoop_rust_open_driver_midi_port, &m)?)?;
    m.add_function(wrap_pyfunction!(objects::shoop_rust_open_driver_decoupled_midi_port, &m)?)?;
    Ok(m)
}