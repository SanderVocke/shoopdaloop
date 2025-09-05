import QtQuick 6.6
import ShoopDaLoop.PythonLogger
import ShoopDaLoop.Rust

ShoopRustSessionControlHandler {
    id: root
    property var lua_engine : null
    property var installed_on : []
    property var logger : PythonLogger { name: "Frontend.Qml.SessionControlHandler" }

    loop_widget_references : session.loops
    track_control_widget_references : session.tracks.map(t => t.control_widget)
    global_state_registry: registries.state_registry
    selected_loops: {
        // Sort by coordinates to get stable results
        var result = Array.from(session.selected_loops)
        result.sort(function(a,b) {
            if (a.track_idx < b.track_idx) { return -1 }
            else if (a.track_idx > b.track_idx) { return 1 }
            else {
                return a.idx_in_track - b.idx_in_track
            }
        })
        return result
    }
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

    // Register for loop events
    function do_loop_event_callback(loop) {
        let event = {
            'coords': [loop.track_idx, loop.idx_in_track],
            'mode': loop.mode,
            'length': loop.length,
            'selected': loop.selected,
            'targeted': loop.targeted,
        }
        root.on_loop_event(event)
    }
    Connections {
        target: session
        function onOn_loop_change_event(loop) {
            root.do_loop_event_callback(loop)
        }
    }
}