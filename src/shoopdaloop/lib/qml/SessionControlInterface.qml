import QtQuick 6.3
import ShoopDaLoop.PythonControlInterface
import ShoopDaLoop.PythonLogger

PythonControlInterface {
    id: root
    qml_instance: this
    property bool ready: false
    property var session: null

    Component.onCompleted: {
        // Register ourselves as "shoop" in the LUA environment
        scripting_engine.use_context(null)
        scripting_engine.create_lua_qobject_interface_as_global('__shoop_control_interface', root)
        ready = true
    }

    property PythonLogger logger : PythonLogger { name: "Frontend.Session.ControlInterface" }

    property list<var> selected_loop_idxs : session.selected_loops ? session.selected_loops.map((l) => [l.track_idx, l.idx_in_track]) : []
    property var targeted_loop_idx: session.targeted_loop ? [session.targeted_loop.track_idx, session.targeted_loop.idx_in_track] : null

    function select_loops(loop_selector) {
        var rval = []
        if (loop_selector.length == 0) {
            rval = []
        } else if (Array.isArray(loop_selector)) {
            if (loop_selector.length == 0) { rval = [] }
            else {
                if (Array.isArray(loop_selector[0])) {
                    // form [[x, y], [x, y], ...]
                    rval = loop_selector.map((coords) => {
                        if (coords[0] >= 0 && coords[0] < tracks_widget.tracks.length &&
                            coords[1] >= 0 && coords[1] < tracks_widget.tracks[coords[0]].loops.length) {
                            return tracks_widget.tracks[coords[0]].loops[coords[1]];
                        } else {        
                            return null
                        }
                    })
                } else {
                    // form [x, y]
                    rval = [ tracks_widget.tracks[loop_selector[0]].loops[loop_selector[1]] ]
                }
            }
        } else {
            // Form callback:  (loop) => true/false
            for(var t=0; t<tracks_widget.tracks.length; t++) {
                let track = tracks_widget.tracks[t]
                for(var l=0; l<track.loops.length; l++) {
                    let loop = track.loops[l]
                    if(loop_selector(loop)) { rval.push(loop) }
                }
            }
        }
        logger.debug(`Selected loops for selector ${JSON.stringify(loop_selector)}: ${JSON.stringify(rval.map(l => l ? l.obj_id : null))}.`)
        return rval
    }

    function select_single_loop(loop_selector) {
        var handlers = select_loops(loop_selector)
        if (handlers.length != 1) {
            logger.throw_error('Handling loop call: multiple loops yielded while only one expected')
        }
        return handlers[0]
    }

    function select_ports(port_selector) {
        var rval = []
        if (Array.isArray(port_selector)) {
            if (port_selector.length == 0) { rval = [] }
            else {
                // Form [track_idx, port_select_fn]
                rval = tracks_widget.tracks[port_selector[0]].ports.filter((p) => port_selector[1](p))
            }
        } else {
            // Form [port_select_fn]
            tracks_widget.tracks.forEach((t) => {
                rval = rval.concat(t.ports.filter((p) => port_selector(p)).map((p) => p.control_handler))
            })
        }
        logger.debug(`Selected ${rval.length} target port(s).`)
        return rval
    }

    function select_single_port(port_selector) {
        var handlers = select_loops(port_selector)
        if (handlers.length != 1) {
            logger.throw_error('Handling port call: multiple ports yielded while only one expected')
        }
        return handlers[0]
    }

    // Interface overrides
    function loop_count_override(loop_selector) {
        return select_loops(loop_selector).filter(l => l != null).length
    }
    function loop_get_all_override() {
        return select_loops((l) => true).map(((l) => [l.track_idx, l.idx_in_track]))
    }
    function loop_get_which_selected_override() {
        return selected_loop_idxs
    }
    function loop_get_which_targeted_override() {
        return targeted_loop_idx
    }
    function loop_get_by_mode_override(mode) {
        return select_loops((l) => l.mode == mode).map(((l) => [l.track_idx, l.idx_in_track]))
    }
    function loop_get_mode_override(loop_selector) {
        return select_loops(loop_selector).map((l) => l.mode)
    }
    function loop_get_length_override(loop_selector) {
        return select_loops(loop_selector).map((l) => l.length)
    }
    function loop_get_by_track_override(track_idx) {
        return select_loops((l) => l.track_idx == track_idx).map(((l) => [l.track_idx, l.idx_in_track]))
    }
    function loop_transition_override(loop_selector, mode, cycles_delay) {
        select_loops(loop_selector).forEach((h) => { h.transition(mode, cycles_delay, state_registry.get('sync_active'), false) } )
    }
    function loop_get_volume_override(loop_selector) {
        return select_single_loop(loop_selector).loop_get_volume(loop_selector)
    }
    function loop_get_balance_override(loop_selector) {
        return select_single_loop(loop_selector).loop_get_balance(loop_selector)
    }
    function loop_record_n_override(loop_selector, n, cycles_delay) {
        select_loops(loop_selector).forEach((h) => { h.record_n(cycles_delay, n) } )
    }
    function loop_record_with_targeted_override(loop_selector) {
        if (targeted_loop_idx) {
            select_loops(loop_selector).forEach((l) => l.record_with_targeted() )
        }
    }
    function loop_set_volume_override(loop_selector, volume) {
        select_loops(loop_selector).forEach((h) => { h.loop_set_volume(loop_selector, volume) } )
    }
    function loop_set_balance_override(loop_selector, balance) {
        select_loops(loop_selector).forEach((h) => { h.loop_set_balance(loop_selector, balance) } )
    }
    function loop_select_override(loop_selector, deselect_others) {
        var selection = new Set(select_loops(loop_selector).map((l) => l ? l.obj_id : null))
        selection.delete(null)
        if (!deselect_others && session.selected_loop_ids) {
            session.selected_loop_ids.forEach((id) => { selection.add(id) })
        }
        state_registry.replace('selected_loop_ids', selection)
    }
    function loop_target_override(loop_selector) {
        for(const loop of select_loops(loop_selector)) {
            if (loop) {
                state_registry.replace('targeted_loop', loop)
                return
            }
        }
        state_registry.replace('targeted_loop_id', null)
    }
    function loop_clear_override(loop_selector) {
        select_loops(loop_selector).forEach((h) => { h.clear() } )
    }
    function loop_untarget_all_override() {
        state_registry.replace('targeted_loop', null)
    }
    function loop_toggle_selected_override(loop_selector) {
        select_loops(loop_selector).forEach((h) => { h.loop_toggle_selected(loop_selector) } )
    }
    function loop_toggle_targeted_override(loop_selector) {
        select_loops(loop_selector).forEach((h) => { h.loop_toggle_targeted(loop_selector) } )
    }
    function port_get_volume_override(port_selector) {
        return select_ports(port_selector).port_get_volume(port_selector)
    }
    function port_get_muted_override(port_selector) {
        return select_ports(port_selector).port_get_muted(port_selector)
    }
    function port_get_input_muted_override(port_selector) {
        return select_ports(port_selector).port_get_input_muted(port_selector)
    }
    function port_mute_override(port_selector) {
        select_ports(port_selector).port_mute(port_selector)
    }
    function port_mute_input_override(port_selector) {
        select_ports(port_selector).port_mute_input(port_selector)
    }
    function port_unmute_override(port_selector) {
        select_ports(port_selector).port_unmute(port_selector)
    }
    function port_unmute_input_override(port_selector) {
        select_ports(port_selector).port_unmute_input(port_selector)
    }
    function port_set_volume_override(port_selector, vol) {
        select_ports(port_selector).port_set_volume(port_selector, vol)
    }
}