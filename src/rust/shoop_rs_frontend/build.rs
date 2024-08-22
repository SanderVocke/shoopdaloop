use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new()
        .qt_module("Network")
        .qt_module("Test")
        .qml_module(QmlModule {
            uri: "shoopdaloop",
            rust_files: &[
                "src/cxx_qt_lib_shoop/rust/qjsonobject.rs",
                "src/cxx_qt_lib_shoop/rust/qlinef.rs",
                "src/cxx_qt_lib_shoop/rust/qobject.rs",
                "src/cxx_qt_lib_shoop/rust/qpen.rs",
                "src/cxx_qt_lib_shoop/rust/qquickitem.rs",
                "src/cxx_qt_lib_shoop/rust/qsignalspy.rs",
                "src/cxx_qt_lib_shoop/rust/qtimer.rs",
                "src/cxx_qt_lib_shoop/rust/qvariant_helpers.rs",
                "src/cxx_qt_lib_shoop/rust/qvariant_qvariantmap.rs",

                "src/cxx_qt_shoop/rust/qobj_autoconnect_bridge.rs",
                "src/cxx_qt_shoop/rust/qobj_backend_wrapper_bridge.rs",
                "src/cxx_qt_shoop/rust/qobj_file_io_bridge.rs",
                "src/cxx_qt_shoop/rust/qobj_find_parent_item_bridge.rs",
                "src/cxx_qt_shoop/rust/qobj_os_utils_bridge.rs",
                "src/cxx_qt_shoop/rust/qobj_release_focus_notifier_bridge.rs",
                "src/cxx_qt_shoop/rust/qobj_render_audio_waveform_bridge.rs",

                "src/cxx_qt_shoop/rust/type_shoop_rust_callable.rs",

                "src/cxx_qt_shoop/rust/test/qobj_generic_test_item_bridge.rs",
                "src/cxx_qt_shoop/rust/test/qobj_test_backend_wrapper_bridge.rs",
                "src/cxx_qt_shoop/rust/test/qobj_test_port_bridge.rs",

                "src/shoop_rs_macros_tests/shoop_rs_macros_test.rs"
            ],
            qml_files: &[] as &[&'static str],
            ..Default::default()
        })
        .cc_builder(|cc| {
            cc.include("src/cxx_qt_shoop/include");
            cc.file("src/cxx_qt_shoop/cxx/ShoopQObject.cpp");
            cc.file("src/cxx_qt_shoop/cxx/shoop_rust_callable.cpp");

            cc.include("src/cxx_qt_lib_shoop/include");
            cc.file("src/cxx_qt_lib_shoop/cxx/qlinef.cpp");
            cc.file("src/cxx_qt_lib_shoop/cxx/qpen.cpp");
        })
        .qobject_header("src/cxx_qt_shoop/include/cxx-qt-shoop/ShoopQObject.h")
        .qt_module("Quick")
        .qt_module("Gui")
        .build();
}