use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new()
        .qt_module("Network")
        .qml_module(QmlModule {
            uri: "shoopdaloop",
            rust_files: &[
                "src/shoop_rs_qobj/qobj_os_utils.rs",
                "src/shoop_rs_qobj/qobj_file_io.rs",

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