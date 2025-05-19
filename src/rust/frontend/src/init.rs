fn register_qml_types_and_singletons() {
    use crate::cxx_qt_shoop::*;

    let mdl = String::from("ShoopDaLoop.Rust");

    // Singletons
    qobj_file_io::register_qml_singleton (&mdl, "ShoopFileIO");
    qobj_os_utils::register_qml_singleton (&mdl, "ShoopOSUtils");
    qobj_release_focus_notifier::register_qml_singleton (&mdl, "ShoopReleaseFocusNotifier");

    // Types
    qobj_render_audio_waveform::register_qml_type(&mdl, "ShoopRenderAudioWaveform");
    qobj_autoconnect::register_qml_type(&mdl, "ShoopAutoConnect");
}

fn register_metatypes() {
    crate::cxx_qt_shoop::type_shoop_rust_callable::register_metatype("ShoopRustCallable");
}

#[no_mangle]
pub extern "C" fn shoop_rust_init() {
    register_metatypes();
    register_qml_types_and_singletons();
}

#[no_mangle]
pub unsafe extern "C" fn shoop_rust_create_autoconnect() -> *mut std::ffi::c_void {
    crate::cxx_qt_shoop::qobj_autoconnect::make_raw() as *mut std::ffi::c_void
}