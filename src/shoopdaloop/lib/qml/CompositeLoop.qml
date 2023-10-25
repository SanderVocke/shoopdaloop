import QtQuick 6.3
import ShoopDaLoop.PythonLogger

import '../generated/types.js' as Types
import '../mode_helpers.js' as ModeHelpers

Item {
    id: root

    // Store the current playback iteration.
    property int iteration: 0
    onIterationChanged: root.logger.trace(() => `iteration: ${iteration}`)

    property var initial_composition_descriptor: null

    Component.onCompleted: root.logger.error(`${JSON.stringify(initial_composition_descriptor)}`)
    onInitial_composition_descriptorChanged: root.logger.error(`${JSON.stringify(initial_composition_descriptor)}`)

    // The sequence is stored as a set of "playlists". Each playlist represents a parallel
    // timeline, stored as a list of { delay: int, loop_id: str }, where the delay can be
    // used to insert blank spots in-between sequential loops.
    property var playlists: (initial_composition_descriptor && initial_composition_descriptor.playlists) ?
        initial_composition_descriptor.playlists : []

    readonly property PythonLogger logger: PythonLogger { name: "Frontend.Qml.CompositeLoop" }

    signal cycled()

    function add_loop(loop, delay, playlist_idx=0) {
        root.logger.debug(`Adding loop ${loop.obj_id} to playlist ${playlist_idx} with delay ${delay}`)
        while (playlist_idx >= playlists.length) {
            playlists.push([])
        }
        playlists[playlist_idx].push({loop_id: loop.obj_id, delay: delay})
        playlistsChanged()
    }

    // Length of a single master loop cycle
    readonly property int cycle_length: master_loop ? master_loop.length : 0

    // Transform the playlists into a more useful format:
    // a dict of { iteration: { loops_start: [...], loops_end: [...], loops_ignored: [...] } }
    // loops_start lists the loops that should be triggered in this iteration.
    // loops_end lists the loops that should be stopped in this iteration.
    // loops_ignored lists loops that are not really scheduled but stored to still keep traceability -
    // for example when a 0-length loop is in the playlist.
    property var schedule: calculate_schedule()
    function calculate_schedule() {
        if (!cycle_length) { return {} }
        var rval = {}
        for (var pidx=0; pidx < playlists.length; pidx++) {
            let playlist = playlists[pidx]
            var _it
            _it = 0
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

                if (!rval[loop_start]) { rval[loop_start] = { loops_start: new Set(), loops_end: new Set(), loops_ignored: new Set() } }
                if (!rval[loop_end]) { rval[loop_end] = { loops_start: new Set(), loops_end: new Set(), loops_ignored: new Set() } }

                if (loop_cycles > 0) {
                    rval[loop_start].loops_start.add(loop)
                    rval[loop_end].loops_end.add(loop)
                } else {
                    rval[loop_start].loops_ignored.add(loop)
                }

                _it += elem.delay + loop_cycles
            }
        }
        root.logger.trace(() => `full schedule:\n${
            Array.from(Object.entries(rval)).map(([k,v]) => 
                `- ${k}: stop [${Array.from(v.loops_end).map(l => l.obj_id)}], start [${Array.from(v.loops_start).map(l => l.obj_id)}], ignore [${Array.from(v.loops_ignored).map(l => l.obj_id)}]`
            ).join("\n")
        }`)
        return rval
    }
    function update_schedule() {
        if (!schedule_frozen) { schedule = calculate_schedule() }
    }
    // During recording, we need to freeze the schedule. Otherwise, the changing lenghts of the recording sub-loop(s) will lead to a cyclic
    // change in the schedule while recording is ongoing.
    readonly property bool schedule_frozen: (ModeHelpers.is_recording_mode(mode) || (ModeHelpers.is_recording_mode(next_mode) && next_transition_delay === 0))
    readonly property bool master_empty: !(cycle_length > 0)
    onPlaylistsChanged: update_schedule()
    onMaster_emptyChanged: update_schedule()

    // Calculated properties
    readonly property int n_cycles: schedule ? Math.max(...Object.keys(schedule)) : 0
    readonly property int master_position: master_loop ? master_loop.position : 0
    readonly property int length: n_cycles * cycle_length
    readonly property int position: iteration * cycle_length + (ModeHelpers.is_running_mode(mode) ? master_position : 0)
    readonly property int display_position : position

    property int mode : Types.LoopMode.Stopped
    property int next_mode : Types.LoopMode.Stopped
    property int next_transition_delay : -1

    property var all_loop_ids: {
        var r = new Set()
        for(var i = 0; i < playlists.length; i++) {
            for (var j = 0; j < playlists[i].length; j++) {
                r.add(playlists[i][j].loop_id)
            }
        }
        return r
    }
    property var all_loops: new Set(Array.from(all_loop_ids || []).map(id => registries.objects_registry.value_or(id, undefined)).filter(m => m))
    property bool all_loops_found:  all_loops.size == all_loop_ids.size

    function print_loops_found() { root.logger.debug(`All ${all_loop_ids.size} loops found: ${all_loops_found}`) }
    onAll_loops_foundChanged: print_loops_found()
    onAll_loop_idsChanged: print_loops_found()

    // If we didn't find all our loops, listen for registry changes to find them later
    Connections {
        target: all_loops_found ? null : registries.objects_registry
        function onItemAdded(id, val) { if (all_loop_ids.has(id)) { playlistsChanged() } }
    }

    RegistryLookup {
        id: master_loop_lookup
        registry: registries.state_registry
        key: 'master_loop'
    }
    property alias master_loop : master_loop_lookup.object

    property var running_loops : new Set()

    function do_triggers(iteration, mode) {
        root.logger.trace(() => `do_triggers(${iteration}, ${mode})`)
        if (iteration in schedule) {
            let elem = schedule[iteration]
            for (var loop of elem.loops_end) {
                loop.transition(Types.LoopMode.Stopped, 0, true, false)
                running_loops.delete(loop)
            }
            if (ModeHelpers.is_running_mode(mode)) {
                for (var loop of elem.loops_start) {
                    // Special case is if we are recording. In that case, the same loop may be scheduled to
                    // record multiple times. The desired behavior is to just record it once and then stop it.
                    // That allows the artist to keep playing to fill the gap if monitoring, or to just stop
                    // playing if not monitoring, and in both cases the resulting recording is the first iteration.
                    var handled = false
                    if (ModeHelpers.is_recording_mode(mode)) {
                        // To implement the above: see if we have already recorded.
                        for(var i=0; i<iteration; i++) {
                            if (i in schedule) {
                                let other_starts = schedule[i].loops_start
                                if (other_starts.has(loop)) {
                                    // We have already recorded this loop. Don't record it again.
                                    root.logger.debug(() => `Not re-recording ${loop.obj_id}`)
                                    loop.transition(Types.LoopMode.Stopped, 0, true, false)
                                    running_loops.delete(loop)
                                    handled = true
                                    break
                                }
                            }
                        }
                    }
                    if (handled) { continue }

                    loop.transition(mode, 0, true, false)
                    running_loops.add(loop)
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
        if (!(n_cycles > 0) && ModeHelpers.is_recording_mode(mode)) {
            // We cannot record a composite loop if the lenghts of the other loops are not yet set.
            return
        }
        if (!ModeHelpers.is_running_mode(root.mode) && ModeHelpers.is_running_mode(mode)) {
            root.iteration = -1
        }
        next_transition_delay = delay
        next_mode = mode
        if (next_transition_delay == 0) {
            if (ModeHelpers.is_running_mode(mode)) {
                do_triggers(0, next_mode)
            } else {
                cancel_all()
            }
        }
    }

    function handle_transition(mode) {
        next_transition_delay = -1
        if (mode != root.mode) {
            cancel_all()
            root.mode = mode
            if (!ModeHelpers.is_running_mode(mode)) {
                root.iteration = 0
            }
        }
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
            if (ModeHelpers.is_running_mode(mode)) {
                var cycled = false
                iteration += 1
                if (iteration >= n_cycles) {
                    iteration = 0
                    cycled = true
                }
                do_triggers(iteration+1, mode)
                if ((iteration+1) >= n_cycles) {
                    if (ModeHelpers.is_recording_mode(mode)) {
                        // Recording ends next cycle
                        transition(Types.LoopMode.Stopped, 0, true)
                    } else {
                        // Will cycle around - trigger the actions for next cycle
                        do_triggers(0, mode)
                    }
                }
                if (cycled) {
                    root.cycled()
                }
            }
        }
    }
}