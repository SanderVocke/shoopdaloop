use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new()
        .qt_module("Network")
        .qml_module(QmlModule {
            uri: "shoopdaloop",
            rust_files: &[
                "src/qobj/qobj_os_utils.rs",
                "src/qobj/qobj_file_io.rs",
                "src/qobj/qobj_release_focus_notifier.rs",

                "src/shoop_rs_macros_tests/shoop_rs_macros_test.rs",
            ],
            qml_files: &[] as &[&'static str],
            ..Default::default()
        })
        .cc_builder(|cc| {
            cc.include("src/cxx");
            cc.file("src/cxx/cxx-shoop/ShoopQObject.cpp");
        })
        .qobject_header("src/cxx/cxx-shoop/ShoopQObject.h")
        .build();
}