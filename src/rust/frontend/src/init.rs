use common::logging::macros::*;
shoop_log_unit!("Main");

fn register_qml_types_and_singletons() {
    use crate::cxx_qt_shoop::*;

    let mdl = String::from("ShoopDaLoop.Rust");

    // Singletons
    qobj_file_io::register_qml_singleton(&mdl, "ShoopFileIO");
    qobj_os_utils::register_qml_singleton(&mdl, "ShoopOSUtils");
    qobj_release_focus_notifier::register_qml_singleton(&mdl, "ShoopReleaseFocusNotifier");
    qobj_crash_handling::register_qml_singleton(&mdl, "ShoopCrashHandling");
    qobj_schema_validator::register_qml_singleton(&mdl, "ShoopSchemaValidator");
    qobj_global_utils::register_qml_singleton(&mdl, "ShoopGlobalUtils");
    qobj_key_modifiers::register_qml_singleton(&mdl, "ShoopKeyModifiers");
    qobj_click_track_generator::register_qml_singleton(&mdl, "ShoopClickTrackGenerator");
    qobj_settings_io::register_qml_singleton(&mdl, "ShoopSettingsIO");

    // Types
    qobj_dummy_process_helper::register_qml_type(&mdl, "ShoopDummyProcessHelper");
    qobj_render_audio_waveform::register_qml_type(&mdl, "ShoopRenderAudioWaveform");
    qobj_render_midi_sequence::register_qml_type(&mdl, "ShoopRenderMidiSequence");
    qobj_autoconnect::register_qml_type(&mdl, "ShoopAutoConnect");
    qobj_loop_gui::register_qml_type(&mdl, "LoopGui");
    qobj_backend_wrapper::register_qml_type(&mdl, "ShoopBackendWrapper");
    qobj_composite_loop_gui::register_qml_type(&mdl, "CompositeLoopGui");
    qobj_port_gui::register_qml_type(&mdl, "PortGui");
    qobj_fx_chain_gui::register_qml_type(&mdl, "FXChainGui");
    qobj_loop_channel_gui::register_qml_type(&mdl, "LoopChannelGui");
    qobj_async_task::register_qml_type(&mdl, "AsyncTask");
    qobj_lua_engine::register_qml_type(&mdl, "LuaEngine");
    qobj_session_control_handler::register_qml_type(&mdl, "SessionControlHandler");
}

fn register_metatypes() {}

#[no_mangle]
pub extern "C" fn init() {
    debug!("Initializing rust metatypes, types and singletons");
    register_metatypes();
    register_qml_types_and_singletons();
    crate::engine_update_thread::init();
}

#[no_mangle]
pub unsafe extern "C" fn shoop_rust_create_autoconnect() -> *mut std::ffi::c_void {
    crate::cxx_qt_shoop::qobj_autoconnect::make_raw() as *mut std::ffi::c_void
}
