import QtQuick 6.3
import ShoopDaLoop.PythonMidiControlPort

PythonMidiControlPort {
    property var lua_engine: null
    property bool initialized: false

    function initialize_lua() {
        if (lua_engine && !initialized) {
            register_lua_interface(lua_engine)
            initialized = Qt.binding(() => backend_initialized) // Back-end also needs to be ready
        }
    }
    Component.onCompleted: { initialize_lua(); rescan_parents() }
    onLua_engineChanged: initialize_lua()
}