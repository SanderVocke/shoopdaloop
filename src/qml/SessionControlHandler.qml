import QtQuick 6.6
import ShoopDaLoop.PythonLogger
import ShoopDaLoop.Rust

ShoopRustSessionControlHandler {
    id: root
    property var session : null
    property var lua_engine : null
    property var installed_on : []
    property var logger : PythonLogger { name: "Frontend.Qml.SessionControlHandler" }

    loop_references : session.loops
    selected_loops: session.selected_loops
    targeted_loop: registries.state_registry.targeted_loop

    function update_engine() {
        if (root.lua_engine && !root.installed_on.includes(lua_engine)) {
            root.logger.debug(`Installing on ${root.lua_engine}`)
            root.install_on_lua_engine(lua_engine) 
            root.installed_on.push(lua_engine)
        }
    }

    Component.onCompleted: { update_engine() }
    onLua_engineChanged: { update_engine() }
}