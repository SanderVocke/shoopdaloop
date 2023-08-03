import ShoopDaLoop.PythonBackend
import QtQuick 6.3

PythonBackend {
    Component.onCompleted: {
        console.log(backend_type)
        maybe_init()
    }
}