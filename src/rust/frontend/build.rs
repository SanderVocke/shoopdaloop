use cxx_qt_build::CxxQtBuilder;

fn main() {
    // If we're pre-building, don't do anything
    #[cfg(feature = "prebuild")]
    {
        return;
    }

    #[cfg(not(feature = "prebuild"))]
    {
        CxxQtBuilder
            ::new()
            .qt_module("Quick")
            .qt_module("Gui")
            .qt_module("Network")
            .qt_module("Test")
            .file("src/cxx_qt_lib_shoop/rust/connect.rs")
            .file("src/cxx_qt_lib_shoop/rust/invokable.rs")
            .file("src/cxx_qt_lib_shoop/rust/qjsonobject.rs")
            .file("src/cxx_qt_lib_shoop/rust/qobject.rs")
            .file("src/cxx_qt_lib_shoop/rust/qquickitem.rs")
            .file("src/cxx_qt_lib_shoop/rust/qsignalspy.rs")
            .file("src/cxx_qt_lib_shoop/rust/qsharedpointer_qobject.rs")
            .file("src/cxx_qt_lib_shoop/rust/qthread.rs")
            .file("src/cxx_qt_lib_shoop/rust/qtimer.rs")
            .file("src/cxx_qt_lib_shoop/rust/qvariant_helpers.rs")
            .file("src/cxx_qt_lib_shoop/rust/qvariant_qobject.rs")
            .file("src/cxx_qt_lib_shoop/rust/qvariant_qvariantmap.rs")
            .file("src/cxx_qt_lib_shoop/rust/qvariant_qsharedpointer_qobject.rs")
            .file("src/cxx_qt_shoop/rust/qobj_autoconnect_bridge.rs")
            .file("src/cxx_qt_shoop/rust/qobj_backend_wrapper_bridge.rs")
            .file("src/cxx_qt_shoop/rust/qobj_dummy_process_helper_bridge.rs")
            .file("src/cxx_qt_shoop/rust/qobj_file_io_bridge.rs")
            .file("src/cxx_qt_shoop/rust/qobj_find_parent_item_bridge.rs")
            .file("src/cxx_qt_shoop/rust/qobj_loop_backend_bridge.rs")
            .file("src/cxx_qt_shoop/rust/qobj_loop_gui_bridge.rs")
            .file("src/cxx_qt_shoop/rust/qobj_os_utils_bridge.rs")
            .file("src/cxx_qt_shoop/rust/qobj_release_focus_notifier_bridge.rs")
            .file("src/cxx_qt_shoop/rust/qobj_render_audio_waveform_bridge.rs")
            .file("src/cxx_qt_shoop/rust/qobj_update_thread_bridge.rs")
            .file("src/cxx_qt_shoop/rust/type_shoop_rust_callable.rs")
            .file("src/cxx_qt_shoop/rust/test/qobj_generic_test_item_bridge.rs")
            .file("src/cxx_qt_shoop/rust/test/qobj_test_backend_wrapper_bridge.rs")
            .file("src/cxx_qt_shoop/rust/test/qobj_test_port_bridge.rs")
            .cc_builder(|cc| {
                cc.include("src/cxx_qt_shoop/include");
                cc.file("src/cxx_qt_shoop/cxx/ShoopQObject.cpp");
                cc.file("src/cxx_qt_shoop/cxx/shoop_rust_callable.cpp");
                cc.include("src/cxx_qt_lib_shoop/include");
            })
            .qobject_header("src/cxx_qt_shoop/include/cxx-qt-shoop/ShoopQObject.h")
            .build();
    }
}