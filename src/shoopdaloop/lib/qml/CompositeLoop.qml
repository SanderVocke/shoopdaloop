import QtQuick 6.3
import ShoopDaLoop.PythonLogger

import '../generated/types.js' as Types
import '../mode_helpers.js' as ModeHelpers

Item {
    id: root
    // The playlist is stored as a list of loop references with a delay that can be used to insert silence.
    // The delay is expressed in master loop cycles.
    property var playlist: []
    property int iteration: -1

    readonly property PythonLogger logger: PythonLogger { name: "Frontend.Qml.CompositeLoop" }

    signal cycled()

    function add_loop(loop, delay) {
        root.logger.debug("Adding loop " + loop.obj_id + " with delay " + delay)
        playlist.push({loop: loop, delay: delay})
        playlistChanged()
    }

    readonly property int cycle_length: master_loop ? master_loop.length : 0
    readonly property int master_position: master_loop ? master_loop.position : 0
    readonly property int n_cycles: {
        var total = 0
        for(var i = 0; i < playlist.length; i++) {
            total += playlist[i].delay
            let loop_n_cycles = Math.ceil(playlist[i].loop.length / cycle_length)
            total += loop_n_cycles
        }
        return total
    }
    readonly property int length: n_cycles * cycle_length
    readonly property int position: iteration * cycle_length + master_position
    property int mode : Types.LoopMode.Stopped
    property int next_mode : Types.LoopMode.Stopped
    property int next_transition_delay : -1

    property var all_loops: {
        var r = new Set()
        for(var i = 0; i < playlist.length; i++) {
            if(playlist[i].loop) {
                r.add(playlist[i].loop)
            }
        }
        return r
    }

    RegistryLookup {
        id: master_loop_lookup
        registry: registries.state_registry
        key: 'master_loop'
    }
    property alias master_loop : master_loop_lookup.object

    function do_triggers(iteration, trigger_loop_begin_fn, trigger_loop_end_fn, recursed = false) {
        var _it = 0
        for(var i = 0; i < playlist.length; i++) {
            let loop_start = _it + playlist[i].delay
            let loop_cycles =  Math.ceil(playlist[i].loop.length / cycle_length)
            let loop_end = loop_start + loop_cycles
            root.logger.debug(`iteration: ${iteration}, loop_start: ${loop_start}, loop_end: ${loop_end}`)
            if ((iteration + 1) == loop_end) {
                root.logger.debug(`triggering loop end for ${playlist[i].loop.obj_id}`)
                trigger_loop_end_fn(playlist[i].loop)
            }
            if ((iteration + 1) == loop_start) {
                root.logger.debug(`triggering loop start for ${playlist[i].loop.obj_id}`)
                trigger_loop_begin_fn(playlist[i].loop)
                break
            }
            _it += playlist[i].delay + loop_cycles
        }
        if ((iteration + 1) >= n_cycles) {
            if (next_transition_delay != 0) {
                // We need to restart in the same mode as before.
                if (recursed) {
                    root.logger.error("Unwanted double recursion")
                    return
                }
                do_triggers(-1, trigger_loop_begin_fn, () => {}, true)
            } else {
                // TODO special case
                root.logger.error("TODO special case")
            }
        }
    }

    function transition(mode, delay, wait_for_sync) {
        if (ModeHelpers.is_running_mode(mode)) {
            iteration = -1
            root.mode = mode
            do_triggers(-1,
                        (loop) => loop.transition(mode, 0, wait_for_sync, false),
                        (loop) => loop.transition(Types.LoopMode.Stopped, 0, wait_for_sync, false)
                        )
        }
    }

    Connections {
        target: master_loop
        function onCycled() {
            if (ModeHelpers.is_running_mode(root.mode)) {
                // Increase our iteration.
                root.iteration++
                if (root.iteration >= n_cycles) {
                    // We've reached the end of the playlist.
                    root.iteration = 0
                    if (next_transition_delay == 0) {
                        root.logger.error("Handle special case")
                        next_transition_delay = -1
                    }
                    if (next_transition_delay > 0) {
                        next_transition_delay -= 1
                    }
                    root.cycled()
                }
                // Do any triggers for the new iteration.
                root.do_triggers(root.iteration,
                                 (loop) => loop.transition(root.mode, 0, true, false),
                                 (loop) => loop.transition(Types.LoopMode.Stopped, 0, true, false)
                                 )
            }
        }
    }
}