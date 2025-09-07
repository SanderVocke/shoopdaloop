import QtQuick 6.6
import ShoopDaLoop.Rust

ShoopRustSessionControlHandler {
    id: root
    property var logger : ShoopRustLogger { name: "Frontend.Qml.SessionControlHandler" }

    loop_widget_references: session.loops
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

    // Observe loop events
    Repeater { 
        model: session.loops
        Item {
            property var mapped_item: session.loops[index]
            Connections {
                target: mapped_item
                function onModeChanged() { root.do_loop_event_callback(mapped_item, ShoopRustConstants.LoopEventType.ModeChanged) }
                function onLengthChanged() { root.do_loop_event_callback(mapped_item, ShoopRustConstants.LoopEventType.LengthChanged) }
                function onSelectedChanged() { root.do_loop_event_callback(mapped_item, ShoopRustConstants.LoopEventType.SelectedChanged) }
                function onTargetedChanged() { root.do_loop_event_callback(mapped_item, ShoopRustConstants.LoopEventType.TargetedChanged) }
                function onIdx_in_trackChanged() { root.do_loop_event_callback(mapped_item, ShoopRustConstants.LoopEventType.CoordsChanged) }
                function onTrack_idxChanged() { root.do_loop_event_callback(mapped_item, ShoopRustConstants.LoopEventType.CoordsChanged) }
            }
        }
    }
    function do_loop_event_callback(loop, type) {
        let event = {
            'coords': [loop.track_idx, loop.idx_in_track],
            'type': type,
            'mode': loop.mode,
            'length': loop.length,
            'selected': loop.selected,
            'targeted': loop.targeted,
        }
        root.on_loop_event(event)
    }

    // Observe global events
    Connections {
        target: registries.state_registry
        function onSolo_activeChanged() { root.do_global_event_callback(ShoopRustConstants.GlobalEventType.GlobalControlChanged) }
        function onSync_activeChanged() { root.do_global_event_callback(ShoopRustConstants.GlobalEventType.GlobalControlChanged) }
        function onPlay_after_record_activeChanged() { root.do_global_event_callback(ShoopRustConstants.GlobalEventType.GlobalControlChanged) }
        function onApply_n_cyclesChanged() { root.do_global_event_callback(ShoopRustConstants.GlobalEventType.GlobalControlChanged) }
    }
    function do_global_event_callback(type) {
        let event = {
            'type': type,
        }
        root.on_global_event(event)
    }

    // Observe key events
    Connections {
        target: session
        function onKey_pressed(event) {
            root.on_key_event({
                'type': ShoopRustConstants.KeyEventType.Pressed,
                'key': event.key,
                'modifiers': event.modifiers
            })
        }
        function onKey_released(event) {
            root.on_key_event({
                'type': ShoopRustConstants.KeyEventType.Released,
                'key': event.key,
                'modifiers': event.modifiers
            })
        }
    }
}