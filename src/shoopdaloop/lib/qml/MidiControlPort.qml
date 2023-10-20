import QtQuick 6.3
import ShoopDaLoop.PythonMidiControlPort

PythonMidiControlPort {
    Component.onCompleted: register_lua_interface(scripting_engine)
}