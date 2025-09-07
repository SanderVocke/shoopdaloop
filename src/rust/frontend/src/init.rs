use common::logging::macros::*;
use config::config::ShoopConfig;
use once_cell::sync::OnceCell;
shoop_log_unit!("Main");

pub static GLOBAL_CONFIG: OnceCell<ShoopConfig> = OnceCell::new();

fn register_qml_types_and_singletons() {
    use crate::cxx_qt_shoop::*;

    let mdl = String::from("ShoopDaLoop.Rust");

    // Singletons
    qobj_file_io::register_qml_singleton(&mdl, "ShoopRustFileIO");
    qobj_os_utils::register_qml_singleton(&mdl, "ShoopRustOSUtils");
    qobj_release_focus_notifier::register_qml_singleton(&mdl, "ShoopRustReleaseFocusNotifier");
    qobj_crash_handling::register_qml_singleton(&mdl, "ShoopRustCrashHandling");
    qobj_schema_validator::register_qml_singleton(&mdl, "ShoopRustSchemaValidator");
    qobj_global_utils::register_qml_singleton(&mdl, "ShoopRustGlobalUtils");
    qobj_key_modifiers::register_qml_singleton(&mdl, "ShoopRustKeyModifiers");
    qobj_click_track_generator::register_qml_singleton(&mdl, "ShoopRustClickTrackGenerator");
    qobj_settings_io::register_qml_singleton(&mdl, "ShoopRustSettingsIO");
    qobj_enums::register_qml_singleton(&mdl, "ShoopRustConstants");
    qobj_test_screen_grabber::register_qml_singleton(&mdl, "ShoopRustTestScreenGrabber");

    // Types
    qobj_dummy_process_helper::register_qml_type(&mdl, "ShoopRustDummyProcessHelper");
    qobj_render_audio_waveform::register_qml_type(&mdl, "ShoopRustRenderAudioWaveform");
    qobj_render_midi_sequence::register_qml_type(&mdl, "ShoopRustRenderMidiSequence");
    qobj_autoconnect::register_qml_type(&mdl, "ShoopRustAutoconnect");
    qobj_loop_gui::register_qml_type(&mdl, "ShoopRustLoopGui");
    qobj_backend_wrapper::register_qml_type(&mdl, "ShoopRustBackendWrapper");
    qobj_composite_loop_gui::register_qml_type(&mdl, "ShoopRustCompositeLoopGui");
    qobj_port_gui::register_qml_type(&mdl, "ShoopRustPortGui");
    qobj_fx_chain_gui::register_qml_type(&mdl, "ShoopRustFXChainGui");
    qobj_loop_channel_gui::register_qml_type(&mdl, "ShoopRustLoopChannelGui");
    qobj_async_task::register_qml_type(&mdl, "ShoopRustAsyncTask");
    qobj_lua_engine::register_qml_type(&mdl, "ShoopRustLuaEngine");
    qobj_session_control_handler::register_qml_type(&mdl, "ShoopRustSessionControlHandler");
    qobj_midi_control_port::register_qml_type(&mdl, "ShoopRustMidiControlPort");
    qobj_logger::register_qml_type(&mdl, "ShoopRustLogger");
}

fn register_metatypes() {}

#[no_mangle]
pub extern "C" fn init(config: &ShoopConfig) {
    debug!("Initializing rust metatypes, types and singletons");
    GLOBAL_CONFIG.set(config.clone()).unwrap();
    register_metatypes();
    register_qml_types_and_singletons();
    crate::engine_update_thread::init();
}

#[no_mangle]
pub unsafe extern "C" fn shoop_rust_create_autoconnect() -> *mut std::ffi::c_void {
    crate::cxx_qt_shoop::qobj_autoconnect::make_raw() as *mut std::ffi::c_void
}
