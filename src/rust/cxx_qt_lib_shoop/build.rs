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
        .file("src/rust/connect.rs")
        .file("src/rust/invokable.rs")
        .file("src/rust/qjsonobject.rs")
        .file("src/rust/qmetatype_helpers.rs")
        .file("src/rust/qobject.rs")
        .file("src/rust/qpointer.rs")
        .file("src/rust/qqmlerror.rs")
        .file("src/rust/qqmlerrorlist.rs")
        .file("src/rust/qquickitem.rs")
        .file("src/rust/qsignalspy.rs")
        .file("src/rust/qsharedpointer_qobject.rs")
        .file("src/rust/qthread.rs")
        .file("src/rust/qtimer.rs")
        .file("src/rust/qvariant_helpers.rs")
        .file("src/rust/qweakpointer_qobject.rs")
        .cc_builder(|cc| {
            cc.include("src/include");
            cc.file("src/cxx/qqmlengine_helpers.cpp");
        })
        .build();
}
