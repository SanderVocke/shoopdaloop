use cxx_qt_build::CxxQtBuilder;

fn main() {
    // If we're pre-building, don't do anything
    if cfg!(feature = "prebuild") {
        return;
    }

    CxxQtBuilder::new()
        .qt_module("Quick")
        .qt_module("Gui")
        .qt_module("Network")
        .qt_module("Test")
        .qt_module("Qml")
        .qt_module("Widgets")
        .file("src/cxx_qt_shoop/rust/fn_qml_debugging.rs")
        .file("src/cxx_qt_shoop/rust/fn_window_icons.rs")
        .file("src/cxx_qt_shoop/rust/qobj_application_bridge.rs")
        .file("src/cxx_qt_shoop/rust/qobj_autoconnect_bridge.rs")
        .file("src/cxx_qt_shoop/rust/qobj_backend_wrapper_bridge.rs")
        .file("src/cxx_qt_shoop/rust/qobj_composite_loop_backend_bridge.rs")
        .file("src/cxx_qt_shoop/rust/qobj_composite_loop_gui_bridge.rs")
        .file("src/cxx_qt_shoop/rust/qobj_crash_handling_bridge.rs")
        .file("src/cxx_qt_shoop/rust/qobj_dummy_process_helper_bridge.rs")
        .file("src/cxx_qt_shoop/rust/qobj_file_io_bridge.rs")
        .file("src/cxx_qt_shoop/rust/qobj_find_parent_item_bridge.rs")
        .file("src/cxx_qt_shoop/rust/qobj_global_utils_bridge.rs")
        .file("src/cxx_qt_shoop/rust/qobj_loop_backend_bridge.rs")
        .file("src/cxx_qt_shoop/rust/qobj_loop_gui_bridge.rs")
        .file("src/cxx_qt_shoop/rust/qobj_os_utils_bridge.rs")
        .file("src/cxx_qt_shoop/rust/qobj_port_gui_bridge.rs")
        .file("src/cxx_qt_shoop/rust/qobj_qmlengine_bridge.rs")
        .file("src/cxx_qt_shoop/rust/qobj_release_focus_notifier_bridge.rs")
        .file("src/cxx_qt_shoop/rust/qobj_render_audio_waveform_bridge.rs")
        .file("src/cxx_qt_shoop/rust/qobj_schema_validator_bridge.rs")
        .file("src/cxx_qt_shoop/rust/qobj_update_thread_bridge.rs")
        .file("src/cxx_qt_shoop/rust/test/qobj_generic_test_item_bridge.rs")
        .file("src/cxx_qt_shoop/rust/test/qobj_test_backend_wrapper_bridge.rs")
        .file("src/cxx_qt_shoop/rust/test/qobj_test_port_bridge.rs")
        .file("src/cxx_qt_shoop/rust/test/qobj_test_file_runner_bridge.rs")
        .file("src/cxx_qt_shoop/rust/type_shoopqmlapplicationengine.rs")
        .cc_builder(|cc| {
            cc.include("src/cxx_qt_shoop/include");
            cc.include("../cxx_qt_lib_shoop/src/include");
            cc.file("src/cxx_qt_shoop/cxx/ShoopQObject.cpp");
            cc.file("src/cxx_qt_shoop/cxx/ShoopQmlEngine.cpp");
        })
        .qobject_header("src/cxx_qt_shoop/include/cxx-qt-shoop/ShoopQObject.h")
        .qobject_header("src/cxx_qt_shoop/include/cxx-qt-shoop/ShoopQmlEngine.h")
        .build();
}
