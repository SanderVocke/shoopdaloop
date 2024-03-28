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

    property bool ran: false
    property bool listening: false

    LuaEngine {
        id: engine
        ready: false

        LuaScript {
            id: script
            when: engine && engine.ready && control_interface && control_interface.ready
            lua_engine: engine

            onRanScript: {
                ran = true
                listening = (control_interface.engine_registered(engine))
            }
        }

        function update() {
            if (root.control_interface && root.control_interface.ready) {
                create_lua_qobject_interface_as_global('__shoop_control_interface', root.control_interface)
            }
            ready = true
        }
        Component.onCompleted: update()
        Component.onDestruction: root.control_interface.unregister_lua_engine(engine)
    }

    function stop() {
        engine.stop()
    }
}

