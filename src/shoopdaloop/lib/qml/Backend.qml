import ShoopDaLoop.PythonBackend
import QtQuick 6.3

PythonBackend {
    Component.onCompleted: maybe_init()
    Component.onDestruction: {
        console.log("BACKEND BEING DESTROYED FROM QML")
    }
}