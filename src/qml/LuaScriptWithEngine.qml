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

            property alias script : the_script
            property bool ready : false

            LuaScript {
                id: the_script

                script_code : root.script_code
                script_path : root.script_path
                script_name : root.script_name

                when: the_engine && the_engine.ready && control_interface
                lua_engine: the_engine

                onRanScript: {
                    ran = true
                    listening = (control_interface.engine_is_installed(the_engine))
                }
            }

            Component.onCompleted: {
                root.control_interface.install_on_lua_engine(the_engine)
                ready = true
            }
            Component.onDestruction: root.control_interface.uninstall_lua_engine(the_engine)
        }
    }

    function stop() {
        loader.active = false
    }
    function start() {
        loader.active = true
    }
}

