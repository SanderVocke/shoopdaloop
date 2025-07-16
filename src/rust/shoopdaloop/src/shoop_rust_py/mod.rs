use pyo3::prelude::*;
mod init;
mod objects;

pub fn create_py_module<'py>(py: Python<'py>) -> Result<Bound<'py, PyModule>, PyErr> {
    let m = PyModule::new_bound(py, "shoop_rust")?;
    m.add_function(wrap_pyfunction!(init::shoop_rust_init, &m)?)?;
    m.add_function(wrap_pyfunction!(
        init::shoop_rust_init_engine_update_thread,
        &m
    )?)?;
    m.add_function(wrap_pyfunction!(
        objects::shoop_rust_add_loop_audio_channel,
        &m
    )?)?;
    m.add_function(wrap_pyfunction!(
        objects::shoop_rust_add_loop_midi_channel,
        &m
    )?)?;
    m.add_function(wrap_pyfunction!(
        objects::shoop_rust_create_autoconnect,
        &m
    )?)?;
    m.add_function(wrap_pyfunction!(objects::shoop_rust_create_fx_chain, &m)?)?;
    m.add_function(wrap_pyfunction!(
        objects::shoop_rust_make_qml_application_engine,
        &m
    )?)?;
    m.add_function(wrap_pyfunction!(
        objects::shoop_rust_open_driver_audio_port,
        &m
    )?)?;
    m.add_function(wrap_pyfunction!(
        objects::shoop_rust_open_driver_midi_port,
        &m
    )?)?;
    m.add_function(wrap_pyfunction!(
        objects::shoop_rust_open_driver_decoupled_midi_port,
        &m
    )?)?;
    m.add_function(wrap_pyfunction!(
        objects::shoop_rust_transition_gui_loop,
        &m
    )?)?;
    m.add_function(wrap_pyfunction!(
        objects::shoop_rust_transition_gui_loops,
        &m
    )?)?;
    m.add_function(wrap_pyfunction!(
        objects::shoop_rust_transition_backend_loop,
        &m
    )?)?;
    m.add_function(wrap_pyfunction!(
        objects::shoop_rust_transition_backend_loops,
        &m
    )?)?;
    m.add_function(wrap_pyfunction!(
        objects::shoop_rust_gui_loop_adopt_ringbuffers,
        &m
    )?)?;
    m.add_function(wrap_pyfunction!(
        objects::shoop_rust_backend_loop_adopt_ringbuffers,
        &m
    )?)?;
    m.add_function(wrap_pyfunction!(
        objects::shoop_rust_get_engine_update_thread_addr,
        &m
    )?)?;
    m.add_function(wrap_pyfunction!(
        objects::shoop_rust_get_engine_update_thread_wrapper_addr,
        &m
    )?)?;
    m.add_function(wrap_pyfunction!(
        objects::shoop_rust_set_crash_json_tag,
        &m
    )?)?;
    m.add_function(wrap_pyfunction!(
        objects::shoop_rust_set_crash_json_toplevel_field,
        &m
    )?)?;

    Ok(m)
}
