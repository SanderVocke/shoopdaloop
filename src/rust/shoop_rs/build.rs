use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new()
        .cc_builder(|cc| {
            cc.include("src/cxx");
        })
        .qt_module("Network")
        .qml_module(QmlModule {
            uri: "shoopdaloop",
            rust_files: &[
                // "src/shoop_rs_qobj/qobj_os_utils.rs",
                "src/shoop_rs_macros_tests/shoop_rs_macros_test.rs",
            ],
            qml_files: &[] as &[&'static str],
            ..Default::default()
        })
        .build();
}