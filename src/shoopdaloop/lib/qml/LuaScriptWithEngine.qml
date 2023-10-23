import QtQuick 6.3

Item {
    id: root
    property var control_interface: null

    // Inputs
    property alias script_code: script.script_code
    property alias script_name: script.script_name
    property alias script_path: script.script_path
    property alias catch_errors: script.catch_errors
    property alias ready: script.ready

    LuaScript {
        id: script
        when: engine.ready
        lua_engine: engine
    }

    LuaEngine {
        id: engine
        ready: false
        function update() {
            if (root.control_interface) {
                create_lua_qobject_interface_as_global('__shoop_control_interface', root.control_interface)
            }
            ready = true
        }
        Component.onCompleted: update()
    }
}

