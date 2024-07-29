use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new()
        .qt_module("Network")
        .qml_module(QmlModule {
            uri: "shoopdaloop",
            rust_files: &[
                "src/shoop_rust_qobj/qobj_os_utils.rs"
            ] as &[&'static str],
            qml_files: &[] as &[&'static str],
            ..Default::default()
        })
        .build();
}