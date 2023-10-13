import QtQuick 6.3
import ShoopDaLoop.PythonControlInterface
import ShoopDaLoop.PythonLogger

PythonControlInterface {
    id: root
    qml_instance: this
    property bool ready: false

    Component.onCompleted: {
        // Register ourselves as "__shoop_control_interface" in the LUA environment
        scripting_engine.use_context(null)
        scripting_engine.create_lua_qobject_interface_as_global('__shoop_control_interface', root)
        ready = true
    }
}