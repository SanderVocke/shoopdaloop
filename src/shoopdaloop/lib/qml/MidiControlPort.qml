import QtQuick 6.3
import ShoopDaLoop.PythonMidiControlPort

PythonMidiControlPort {
    property var lua_engine: null
    property bool initialized: false

    function initialize() {
        if (lua_engine && !initialized) {
            register_lua_interface(lua_engine)
            initialized = true
        }
    }
    Component.onCompleted: initialize()
    onLua_engineChanged: initialize()
}