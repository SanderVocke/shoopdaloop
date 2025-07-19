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

    // Types
    qobj_dummy_process_helper::register_qml_type(&mdl, "ShoopDummyProcessHelper");
    qobj_render_audio_waveform::register_qml_type(&mdl, "ShoopRenderAudioWaveform");
    qobj_autoconnect::register_qml_type(&mdl, "ShoopAutoConnect");
    qobj_loop_gui::register_qml_type(&mdl, "LoopGui");
    qobj_backend_wrapper::register_qml_type(&mdl, "ShoopBackendWrapper");
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
