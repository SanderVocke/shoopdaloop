import ShoopDaLoop.PythonBackend
import QtQuick 6.3

PythonBackend {
    Component.onCompleted: {
        maybe_init()
    }
    Component.onDestruction: close()
}