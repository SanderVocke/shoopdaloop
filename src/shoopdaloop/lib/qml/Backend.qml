import ShoopDaLoop.PythonBackend
import QtQuick 6.6
import ShoopConstants

PythonBackend {
    Component.onCompleted: {
        maybe_init()
    }
    Component.onDestruction: close()

    Connections {
        target: Qt.application
        function onAboutToQuit() { close() }
    }
}