import ShoopDaLoop.PythonBackend
import QtQuick 6.3

PythonBackend {
    backend_argstring: ''
    Component.onCompleted: {
        maybe_init()
    }
    Component.onDestruction: close()
}