import QtQuick 6.6
import ShoopDaLoop.PythonMidiControlPort

PythonMidiControlPort {
    property var lua_engine: null
    property bool all_initialized: false

    function initialize_lua() {
        if (lua_engine && !all_initialized) {
            register_lua_interface(lua_engine)
            all_initialized = Qt.binding(() => initialized) // Back-end also needs to be ready
        }
    }
    Component.onCompleted: { initialize_lua(); rescan_parents() }
    onLua_engineChanged: initialize_lua()

    Component.onDestruction: { close() }
}