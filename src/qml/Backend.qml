import QtQuick 6.6
import ShoopDaLoop.Rust

ShoopRustBackendWrapper {
    objectName: "shoop_backend_wrapper"
    Component.onCompleted: {
        maybe_init()
    }
    Component.onDestruction: close()

    Connections {
        target: Qt.application
        function onAboutToQuit() { close() }
    }
}