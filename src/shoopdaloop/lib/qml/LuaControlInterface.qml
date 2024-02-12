import QtQuick 6.3
import ShoopDaLoop.PythonControlInterface
import ShoopDaLoop.PythonLogger

PythonControlInterface {
    id: root
    qml_instance: this
    property bool ready: introspected_qml_instance == this
}