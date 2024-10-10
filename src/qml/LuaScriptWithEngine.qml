import QtQuick 6.6

Item {
    id: root
    property var control_interface: null

    // Inputs
    property alias script : loader.script
    property alias engine : loader.engine
    property var script_code
    property string script_name
    property var script_path
    property bool catch_errors
    property bool ready: script !== undefined && script.ready !== undefined && script.ready

    property bool ran: false
    property bool listening: false

    Loader {
        id: loader
        active: true

        property var engine : active ? item : undefined
        property var script : active ? item.script : undefined

        sourceComponent : LuaEngine {
            id: the_engine
            ready: false

            property alias script : the_script

            LuaScript {
                id: the_script

                script_code : root.script_code
                script_path : root.script_path
                script_name : root.script_name
                catch_errors : root.catch_errors

                when: the_engine && the_engine.ready && control_interface && control_interface.ready
                lua_engine: the_engine

                onRanScript: {
                    ran = true
                    listening = (control_interface.engine_registered(the_engine))
                }
            }

            function update() {
                if (root.control_interface && root.control_interface.ready) {
                    create_lua_qobject_interface_as_global('__shoop_control_interface', root.control_interface)
                }
                ready = true
            }
            Component.onCompleted: update()
            Component.onDestruction: root.control_interface.unregister_lua_engine(the_engine)
        }
    }

    function stop() {
        loader.active = false
    }
    function start() {
        loader.active = true
    }
}

