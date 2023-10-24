import QtQuick 6.3
import ShoopDaLoop.PythonLogger

import '../generated/types.js' as Types
import '../mode_helpers.js' as ModeHelpers

Item {
    id: root

    // Store the current playback iteration.
    property int iteration: -1

    property var initial_composition_descriptor: null

    // The sequence is stored as a set of "playlists". Each playlist represents a parallel
    // timeline, stored as a list of { delay: int, loop_id: str }, where the delay can be
    // used to insert blank spots in-between sequential loops.
    property var playlists: initial_composition_descriptor.playlists

    readonly property PythonLogger logger: PythonLogger { name: "Frontend.Qml.CompositeLoop" }

    signal cycled()

    function add_loop(loop, delay, playlist_idx=0) {
        root.logger.debug("Adding loop " + loop.obj_id + " with delay " + delay)
        while (playlist_idx >= playlists.length) {
            playlists.push([])
        }
        playlists[playlist_idx].push({loop_id: loop.obj_id, delay: delay})
        playlistsChanged()
    }

    // Length of a single master loop cycle
    readonly property int cycle_length: master_loop ? master_loop.length : 0

    // Transform the playlists into a more useful format:
    // a dict of { iteration: { loops_start: [...], loops_end: [...] } }
    readonly property var schedule: {
        var rval = {}
        for (var pidx=0; pidx < playlists.length; pidx++) {
            let playlist = playlists[pidx]
            var _it = 0
            for (var i=0; i<playlist.length; i++) {
                let elem = playlist[i]
                let loop = registries.objects_registry.value_or(elem.loop_id, undefined)
                if (!loop) {
                    root.logger.warning("Could not find " + elem.loop_id) 
                    continue
                }
                let loop_start = _it + elem.delay
                let loop_cycles =  Math.ceil(loop.length / cycle_length)
                let loop_end = loop_start + loop_cycles

                if (!(loop_start in rval)) { rval[loop_start] = { loops_start: new Set(), loops_end: new Set() } }
                if (!(loop_end in rval)) { rval[loop_end] = { loops_start: new Set(), loops_end: new Set() } }

                rval[loop_start].loops_start.add(loop)
                rval[loop_end].loops_end.add(loop)

                _it += elem.delay + loop_cycles
            }
        }
        return rval
    }

    // Calculated properties
    readonly property int n_cycles: Math.max(...Object.keys(schedule))
    readonly property int master_position: master_loop ? master_loop.position : 0
    readonly property int length: n_cycles * cycle_length
    readonly property int position: iteration * cycle_length + master_position

    property int mode : Types.LoopMode.Stopped
    property int next_mode : Types.LoopMode.Stopped
    property int next_transition_delay : -1

    property var all_loop_ids: {
        var r = new Set()
        for(var i = 0; i < playlists.length; i++) {
            for (var j = 0; j < playlists[i].length; j++) {
                if (playlists[i][j].loop) { r.add(playlists[i][j].loop) }
            }
        }
        return r
    }
    property var all_loops: new Set(Array.from(all_loop_ids || []).map(id => registries.objects_registry.value_or(id, undefined)))

    onAll_loop_idsChanged: root.logger.warning(`ids ${Array.from(all_loop_ids)}`)
    onAll_loopsChanged: root.logger.warning(`loops ${Array.from(all_loops)}`)

    RegistryLookup {
        id: master_loop_lookup
        registry: registries.state_registry
        key: 'master_loop'
    }
    property alias master_loop : master_loop_lookup.object

    property var running_loops : new Set()

    function do_triggers(iteration, mode) {
        if (iteration in schedule) {
            for (var loop_id of schedule[iteration].loops_end) {
                let loop = registries.objects_registry.value_or(loop_id, undefined)
                if (loop) {
                    loop.transition(Types.LoopMode.Stopped, 0, true, false)
                    running_loops.delete(loop)
                }
            }
            for (var loop_id of schedule[iteration].loops_start) {
                let loop = registries.objects_registry.value_or(loop_id, undefined)
                if (loop) {
                    loop.transition(mode, 0, true, false)
                    if (is_running_mode(mode)) {
                        running_loops.add(loop)
                    }
                }
            }
        }
    }

    function cancel_all() {
        for (var loop of running_loops) {
            loop.transition(Types.LoopMode.Stopped, 0, true, false)
        }
        running_loops = new Set()
    }

    function transition(mode, delay, wait_for_sync) {
    }

    function handle_transition(mode) {
        next_transition_delay = -1
        if (mode != root.mode) {
            cancel_all()
            root.mode = mode
        }
        root.logger.warning("retrigger loops")
    }

    function actual_composition_descriptor() {
        return {
            'playlists': playlists
        }
    }

    Connections {
        target: master_loop
        function onCycled() {
            if (next_transition_delay == 0) {
                handle_transition(next_mode)
            }
            if (is_running_mode(mode)) {
                iteration += 1
                do_triggers(iteration)
                if (iteration >= n_cycles) {
                    iteration = 0
                    do_triggers(iteration)
                }
            }
        }
    }
}