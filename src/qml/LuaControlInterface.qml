import QtQuick 6.6
import ShoopDaLoop.PythonControlInterface
import ShoopDaLoop.PythonLogger

PythonControlInterface {
    id: root
    property bool ready: introspected_qml_instance !== null && introspected_qml_instance !== undefined && introspected_qml_instance == qml_instance

    onQml_instanceChanged: introspect()
}