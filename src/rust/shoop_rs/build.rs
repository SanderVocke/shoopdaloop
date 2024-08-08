use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new()
        .qt_module("Network")
        .qml_module(QmlModule {
            uri: "shoopdaloop",
            rust_files: &[
                "src/cxx_qt_lib_shoop/rust/qlinef.rs",
                "src/cxx_qt_lib_shoop/rust/qpen.rs",

                "src/cxx_qt_shoop/rust/shoop_rust_callable.rs",
                "src/cxx_qt_shoop/rust/qobj_os_utils.rs",
                "src/cxx_qt_shoop/rust/qobj_file_io.rs",
                "src/cxx_qt_shoop/rust/qobj_release_focus_notifier.rs",
                "src/cxx_qt_shoop/rust/qobj_render_audio_waveform.rs",

                "src/shoop_rs_macros_tests/shoop_rs_macros_test.rs",
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