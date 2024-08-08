pub mod audio_power_pyramid;
pub mod logging;

pub mod cxx_qt_shoop;
pub mod cxx_qt_lib_shoop;

pub mod shoop_rs_macros_tests;

fn register_qml_types_and_singletons() {
    use cxx_qt_shoop::*;

    let mdl = String::from("ShoopDaLoop.Rust");

    // Singletons
    qobj_file_io::register_qml_singleton (&mdl, "ShoopFileIO");
    qobj_os_utils::register_qml_singleton (&mdl, "ShoopOSUtils");
    qobj_release_focus_notifier::register_qml_singleton (&mdl, "ShoopReleaseFocusNotifier");

    // Types
    qobj_render_audio_waveform::register_qml_type(&mdl, "ShoopRenderAudioWaveform");
}

fn register_metatypes() {
    cxx_qt_shoop::shoop_rust_callable::register_metatype("ShoopRustCallable");
}

#[no_mangle]
pub extern "C" fn shoop_rust_init() {
    logging::init_logging().expect("Unable to initialize shoop_rs logging");

    register_metatypes();
    register_qml_types_and_singletons();
}