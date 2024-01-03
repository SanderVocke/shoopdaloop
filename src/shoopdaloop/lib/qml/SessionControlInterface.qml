import QtQuick 6.3
import ShoopDaLoop.PythonLogger

import ShoopConstants
import '../midi.js' as Midi

LuaControlInterface {
    id: root
    qml_instance: this
    property var session: null

    property PythonLogger logger : PythonLogger { name: "Frontend.Qml.SessionControlInterface" }

    property list<var> selected_loop_idxs : session.selected_loops ? Array.from(session.selected_loops).map((l) => [l.track_idx, l.idx_in_track]) : []
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
                } else if (loop_selector.length == 2) {
                    // form [x, y]
                    let maybe_track = tracks_widget.tracks[loop_selector[0]]
                    rval = maybe_track ?
                        [ tracks_widget.tracks[loop_selector[0]].loops[loop_selector[1]] ] :
                        []
                } else {
                    logger.warning(() => (`Invalid loop selector: ${JSON.stringify(loop_selector)}`))        
                    rval = []
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
        logger.debug(() => (`Selected loops for selector ${JSON.stringify(loop_selector)}: ${JSON.stringify(rval.map(l => l ? l.obj_id : null))}.`))
        return rval
    }

    function select_single_loop(loop_selector) {
        var handlers = select_loops(loop_selector)
        if (handlers.length != 1) {
            logger.throw_error('Handling loop call: multiple loops yielded while only one expected')
        }
        return handlers[0]
    }

    function select_tracks(track_selector) {
        var rval = []
        if (Array.isArray(track_selector)) {
            if (track_selector.length == 0) { rval = [] }
            else {
                // Form [track_idx1, track_idx2, ...]
                rval = track_selector.map((idx) => tracks_widget.tracks[idx]).filter(t => t != null && t != undefined)
            }
        } else if (Number.isInteger(track_selector)) {
            if (tracks_widget.tracks.length > track_selector) {
                return [tracks_widget.tracks[track_selector]]
            } else { return [] }
        } else {
            // Form [track_select_fn]
            rval = tracks_widget.tracks.filter((t) => track_selector(t))
        }
        logger.debug(() => (`Selected ${rval.length} target track(s).`))
        return rval
    }

    function select_single_track(track_selector) {
        var handlers = select_tracks(track_selector)
        if (handlers.length != 1) {
            logger.throw_error('Handling track call: multiple tracks yielded while only one expected')
        }
        return handlers[0]
    }

    // Loop interface overrides
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
    function loop_get_next_mode_override(loop_selector) {
        return select_loops(loop_selector).map((l) => {
            if (l.next_mode !== null && l.next_transition_delay !== null && l.next_transition_delay >= 0) {
                return l.next_mode
            }
            return null
        })
    }
    function loop_get_next_mode_delay_override(loop_selector) {
        return select_loops(loop_selector).map((l) => {
            if (l.next_mode !== null && l.next_transition_delay !== null && l.next_transition_delay >= 0) {
                return l.next_transition_delay
            }
            return null
        })
    }
    function loop_get_length_override(loop_selector) {
        return select_loops(loop_selector).map((l) => l.length)
    }
    function loop_get_by_track_override(track_idx) {
        return select_loops((l) => l.track_idx == track_idx).map(((l) => [l.track_idx, l.idx_in_track]))
    }
    function loop_transition_override(loop_selector, mode, cycles_delay) {
        select_loops(loop_selector).forEach((h) => { h.transition(mode, cycles_delay, registries.state_registry.get('sync_active'), false) } )
    }
    function loop_get_gain_override(loop_selector) {
        return select_loops(loop_selector).map(l => l.last_pushed_gain)
    }
    function loop_get_gain_fader_override(loop_selector) {
        return select_loops(loop_selector).map(l => l.get_gain_fader())
    }
    function loop_get_balance_override(loop_selector) {
        return select_loops(loop_selector).map(l => l.last_pushed_stereo_balance)
    }
    function loop_record_n_override(loop_selector, n, cycles_delay) {
        select_loops(loop_selector).forEach((h) => { h.record_n(cycles_delay, n) } )
    }
    function loop_record_with_targeted_override(loop_selector) {
        if (targeted_loop_idx) {
            select_loops(loop_selector).forEach((l) => l.record_with_targeted() )
        }
    }
    function loop_set_gain_override(loop_selector, gain) {
        select_loops(loop_selector).forEach((h) => { h.push_gain(gain) } )
    }
    function loop_set_gain_fader_override(loop_selector, gain) {
        select_loops(loop_selector).forEach((h) => { h.set_gain_fader(gain) } )
    }
    function loop_set_balance_override(loop_selector, balance) {
        select_loops(loop_selector).forEach((h) => { h.push_stereo_balance(balance) } )
    }
    function loop_select_override(loop_selector, deselect_others) {
        var selection = new Set(select_loops(loop_selector).map((l) => l ? l.obj_id : null))
        selection.delete(null)
        if (!deselect_others && session.selected_loop_ids) {
            session.selected_loop_ids.forEach((id) => { selection.add(id) })
        }
        registries.state_registry.replace('selected_loop_ids', selection)
    }
    function loop_target_override(loop_selector) {
        for(const loop of select_loops(loop_selector)) {
            if (loop) {
                registries.state_registry.replace('targeted_loop', loop)
                return
            }
        }
        registries.state_registry.replace('targeted_loop', null)
    }
    function loop_clear_override(loop_selector) {
        select_loops(loop_selector).forEach((h) => { h.clear() } )
    }
    function loop_untarget_all_override() {
        registries.state_registry.replace('targeted_loop', null)
    }
    function loop_toggle_selected_override(loop_selector) {
        select_loops(loop_selector).forEach((h) => { h.loop_toggle_selected(loop_selector) } )
    }
    function loop_toggle_targeted_override(loop_selector) {
        select_loops(loop_selector).forEach((h) => { h.loop_toggle_targeted(loop_selector) } )
    }

    // Track interface overrides
    function track_set_gain_override(track_selector, vol) {
        select_tracks(track_selector).forEach(t => t.control_widget.set_gain(vol))
    }
    function track_set_gain_fader_override(track_selector, vol) {
        select_tracks(track_selector).forEach(t => t.control_widget.set_gain_fader(vol))
    }
    function track_set_input_gain_override(track_selector, vol) {
        select_tracks(track_selector).forEach(t => t.control_widget.set_input_gain(vol))
    }
    function track_set_input_gain_fader_override(track_selector, vol) {
        select_tracks(track_selector).forEach(t => t.control_widget.set_input_gain_fader(vol))
    }
    function track_get_gain_override(track_selector) {
        return select_tracks(track_selector).map(t => t.control_widget.last_pushed_gain)
    }
    function track_get_gain_fader_override(track_selector) {
        return select_tracks(track_selector).map(t => t.control_widget.gain_fader_position)
    }
    function track_get_input_gain_override(track_selector) {
        return select_tracks(track_selector).map(t => t.control_widget.last_pushed_in_gain)
    }
    function track_get_input_gain_fader_override(track_selector) {
        return select_tracks(track_selector).map(t => t.control_widget.input_fader_position)
    }
    function track_get_muted_override(track_selector) {
        return select_tracks(track_selector).map(t => t.control_widget.mute)
    }
    function track_set_muted_override(track_selector, muted) {
        return select_tracks(track_selector).forEach(t => {t.control_widget.mute = muted})
    }
    function track_get_input_muted_override(track_selector) {
        return select_tracks(track_selector).map(t => !t.control_widget.monitor)
    }
    function track_set_input_muted_override(track_selector, muted) {
        return select_tracks(track_selector).forEach(t => {t.control_widget.monitor = !muted})
    }

    // Handle creation and deletion of dynamic MIDI control ports based on registered connection rules.
    
    // form { rule_id: port }
    property var midi_control_ports: ({})

    readonly property var midi_control_port_factory : Qt.createComponent("MidiControlPort.qml")

    onMidiInputPortRulesChanged: {
        for(var i=0; i<midi_input_port_rules.length; i++) {
            let rule = midi_input_port_rules[i]
            let id = rule.id
            if (!Object.keys(midi_control_ports).includes(id)) {
                if (midi_control_port_factory.status == Component.Error) {
                    root.logger.error("Failed to load MIDI port factory: " + midi_control_port_factory.errorString())
                    return
                } else if (midi_control_port_factory.status != Component.Ready) {
                    root.logger.error("MIDI port factory not ready.")
                    return
                } else {
                    root.logger.debug(`Creating lazy port for autoconnect rule ${rule.id} (${rule.regex}).`)
                    var port = midi_control_port_factory.createObject(root, {
                        "may_open": false,
                        "name_hint": "auto_control_" + rule.id,
                        "autoconnect_regexes": [ rule.regex ],
                        "direction": ShoopConstants.PortDirection.Input,
                        "parent": root,
                        'lua_engine': rule.engine
                    });
                    port.detectedExternalAutoconnectPartnerWhileClosed.connect((p=port) => {
                        if (!p.may_open) {
                            root.logger.debug(`Opening port for autoconnect rule ${rule.id} (${rule.regex}).`)
                            p.may_open = true
                        }
                    })
                    port.msgReceived.connect((msg, cb=rule.msg_cb, p=port) => {
                        p.lua_engine.call(cb, [Midi.parse_msg(msg), null], false) 
                    })
                    midi_control_ports[i] = port
                    midi_control_portsChanged()
                }
            }
        }
    }

    onMidiOutputPortRulesChanged: {
        for(var i=0; i<midi_output_port_rules.length; i++) {
            let rule = midi_output_port_rules[i]
            let id = rule.id
            if (!Object.keys(midi_control_ports).includes(id)) {
                if (midi_control_port_factory.status == Component.Error) {
                    root.logger.error("Failed to load MIDI port factory: " + midi_control_port_factory.errorString())
                    return
                } else if (midi_control_port_factory.status != Component.Ready) {
                    root.logger.error("MIDI port factory not ready.")
                    return
                } else {
                    root.logger.debug(`Creating lazy port for autoconnect rule ${rule.id} (${rule.regex}).`)
                    var port = midi_control_port_factory.createObject(root, {
                        "may_open": false,
                        "name_hint": "auto_control_" + rule.id,
                        "autoconnect_regexes": [ rule.regex ],
                        "direction": ShoopConstants.PortDirection.Output,
                        "parent": root
                    });
                    port.initializedChanged.connect((initialized, cb=rule.opened_cb, p=port) => {
                        if (initialized) {
                            let send_fn = p.get_py_send_fn()
                            p.lua_engine.call(cb, [send_fn], null, true) }
                    })
                    port.connected.connect((cb=rule.connected_cb, p=port) => {
                        if (p.lua_engine) {
                            p.lua_engine.call(cb, [], null, false)
                        } else {
                            p.onLua_engineChanged.connect((p=p, cb=cb) => {
                                p.lua_engine.call(cb, [], null, false)
                            })
                        }
                    })
                    port.detectedExternalAutoconnectPartnerWhileClosed.connect((p=port) => {
                        if (!p.may_open) {
                            root.logger.debug(`Opening port for autoconnect rule ${rule.id} (${rule.regex}).`)
                            p.may_open = true
                        }
                    })
                    midi_control_ports[i] = port
                    midi_control_portsChanged()
                }
            }
        }
    }

    // Gather and forward loop events
    RegistrySelects {
        registry: registries.objects_registry
        select_fn: (obj) => obj && obj.object_schema && obj.object_schema.match(/loop.[0-9]+/)
        id: lookup_loops
        values_only: true
    }

    Repeater { 
        model: lookup_loops.objects
        Item {
            property var mapped_item: lookup_loops.objects[index]
            property list<int> coords: [mapped_item.track_idx, mapped_item.idx_in_track]
            function send_event() {
                let event = {
                    'mode': mapped_item.mode,
                    'length': mapped_item.length,
                    'selected': mapped_item.selected,
                    'targeted': mapped_item.targeted,
                }
                control_interface.loop_event(coords, event)
            }
            Connections {
                target: mapped_item
                function onModeChanged() { send_event() }
                function onLengthChanged() { send_event() }
                function onSelectedChanged() { send_event() }
                function onTargetedChanged() { send_event() }
            }
        }
    }
}