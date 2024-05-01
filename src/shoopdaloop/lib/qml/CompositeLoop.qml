import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6
import ShoopDaLoop.PythonLogger
import ShoopDaLoop.PythonCompositeLoop

import ShoopConstants
import '../mode_helpers.js' as ModeHelpers

Item {
    id: root

    // Store the current playback iteration.
    onIterationChanged: root.logger.trace(() => `iteration -> ${iteration}`)

    property var initial_composition_descriptor: null
    property string obj_id : 'unknown'
    property var loop_widget : null

    onObj_idChanged: py_loop.instanceIdentifier = obj_id

    readonly property bool initialized: true

    // set internally
    property alias kind : py_loop.kind

    // The Python-side object manages triggering other loops based on the
    // schedule. This needs to keep running even if the QML/GUI thread hangs.
    // In the QML side, we manage updating/calculating the schedule.
    property var schedule: {}
    property alias iteration: py_loop.iteration
    property alias running_loops: py_loop.running_loops
    property alias mode: py_loop.mode
    property alias n_cycles: py_loop.n_cycles
    property alias next_mode : py_loop.next_mode
    property alias next_transition_delay : py_loop.next_transition_delay
    property alias sync_length : py_loop.sync_length
    property alias position : py_loop.position
    property alias sync_position : py_loop.sync_position
    property alias length : py_loop.length
    property alias py_loop : py_loop
    PythonCompositeLoop {
        id: py_loop
        iteration: 0
        sync_loop: (root.sync_loop && root.sync_loop.maybe_loop) ? root.sync_loop.maybe_loop : null
        schedule: root.schedule
        play_after_record: registries.state_registry.play_after_record_active
        sync_mode_active: registries.state_registry.sync_active

        onCycled: n => root.cycled(n)
        Component.onCompleted: root.recalculate_schedule()
    }

    // The sequence is stored as a set of "playlists". Each playlist represents a parallel
    // timeline, stored as a list of sublists of:
    // [
    //    { 
    //       delay: int,
    //       loop_id: str,
    //       n_cycles: int | undefined,
    //       mode: loop mode | undefined
    //    }
    // ],
    // where the delay can be used to insert blank spots in-between sequential loops.
    // the n_cycles can be set to force how long the loop is activated in the schedule. If undefined, it will
    // just be determined from the current loop length or assumed to be 1 for empty loops.
    // if the mode is set, the loop will be triggered with that mode always.
    property var playlists_in: (initial_composition_descriptor && initial_composition_descriptor.playlists) ?
        initial_composition_descriptor.playlists : []

    // Copy the incoming playlists, but do some validity checking on them too.
    property var playlists: []
    ExecuteNextCycle {
        id: clear_playlists_in
        onExecute: root.playlists_in = []
    }
    
    onPlaylists_inChanged: {
        let own_obj_id = root.obj_id

        function for_each_loop_id(_playlists, fn) {
            for(var pidx=0; pidx<_playlists.length; pidx++) {
                let playlist = _playlists[pidx]
                for (var i=0; i<playlist.length; i++) {
                    for (var j=0; j<playlist[i].length; j++) {
                        let elem = playlist[i][j]
                        fn(elem.loop_id)
                    }
                }
            }
        }

        var circular = false
        var rval = JSON.parse(JSON.stringify(playlists_in))
        for_each_loop_id(rval, (loop_id) => {
            if (loop_id == own_obj_id) {
                // Circular reference between two composite loops is not allowed.
                root.logger.error(`A self-reference was made in a composite loop: ${own_obj_id}. This is not allowed - clearing the schedule.`)
                circular = true
            }
            if (circular) { return; }
            let loop_widget = registries.objects_registry.value_or(loop_id, undefined)
            if (loop_widget && loop_widget.maybe_composite_loop) {
                for_each_loop_id(loop_widget.maybe_composite_loop.playlists, (lid) => {
                    if (lid == own_obj_id) {
                        root.logger.error(`A circular reference was made between composite loops ${own_obj_id} and ${loop_id}. This is not allowed - clearing schedule of ${own_obj_id}.`)
                        circular = true
                    }
                })
            }
        })
        if (circular) {
            clear_playlists_in.trigger()
        } else {
            playlists = rval
        }
    }

    // When the schedule is calculated, an enriched playlists copy is also calculated.
    // This is equal to the input playlists, just with each entry having its final
    // scheduling information included:
    // { delay: int, loop_id: str, start_iteration: int, end_iteration: int, loop: Loop }
    property var scheduled_playlists: []

    readonly property PythonLogger logger: PythonLogger {
        name: "Frontend.Qml.CompositeLoop"
        instanceIdentifier: obj_id
    }

    signal cycled(int cycle_nr)

    function add_loop(loop, delay, n_cycles=undefined, playlist_idx=undefined) {
        root.logger.debug(`Adding loop ${loop.obj_id} to playlist ${playlist_idx} with delay ${delay}, n_cycles override ${n_cycles}`)
        if (playlist_idx === undefined) {
            playlist_idx = playlists_in.length
        }
        while (playlist_idx >= playlists_in.length) {
            playlists_in.push([])
        }
        playlists_in[playlist_idx].push([{loop_id: loop.obj_id, delay: delay, n_cycles: n_cycles}])
        playlists_inChanged()
    }

    // Transform the playlists into a more useful format:
    // a dict of { iteration: { loops_start: [...], loops_end: [...], loops_ignored: [...] } }
    // loops_start lists the loops that should be triggered in this iteration. Each entry is a [loop, mode].
    // loops_end lists the loops that should be stopped in this iteration. Each entry is a loop.
    // loops_ignored lists loops that are not really scheduled but stored to still keep traceability -
    // for example when a 0-length loop is in the playlist.
    function recalculate_schedule() {
        root.logger.debug(() => 'Recalculating schedule.')
        root.logger.trace(() => `--> playlists to schedule: ${JSON.stringify(playlists, null, 2)}`)

        var _scheduled_playlists = playlists ? JSON.parse(JSON.stringify(playlists)) : []
        for (var pidx=0; pidx < _scheduled_playlists.length; pidx++) {
            let playlist = _scheduled_playlists[pidx]
            var _it
            _it = 0
            for (var i=0; i<playlist.length; i++) {
                let elems = playlist[i]
                var total_duration = 0
                for (var h=0; h<elems.length; h++) {
                    let elem = elems[h]
                    let loop_widget = registries.objects_registry.value_or(elem.loop_id, undefined)
                    if (!loop_widget) {
                        root.logger.debug(() => "Could not find " + elem.loop_id) 
                        continue
                    }
                    let loop_start = _it + elem.delay
                    let loop_cycles =  Math.max(1, 
                        elem.n_cycles ?
                            elem.n_cycles :
                            ((loop_widget.n_cycles > 0 && !ModeHelpers.is_recording_mode(loop_widget.mode)) ?
                                loop_widget.n_cycles : 1
                            )
                    )
                    let loop_end = loop_start + loop_cycles

                    elem['start_iteration'] = loop_start
                    elem['end_iteration'] = loop_end
                    elem['loop_widget'] = loop_widget
                    elem['forced_n_cycles'] = (elem.n_cycles && elem.n_cycles > 0) ? elem.n_cycles : null
                    if (elem['mode'] === undefined) { elem['mode'] = null }

                    let duration = elem.delay + loop_cycles
                    total_duration = Math.max(total_duration, duration)
                }
                _it += total_duration
            }
        }
        // Store the annotated playlists
        scheduled_playlists = _scheduled_playlists

        // Calculate the schedule
        var _schedule = {}
        for(var i=0; i<_scheduled_playlists.length; i++) {
            for(var j=0; j<_scheduled_playlists[i].length; j++) {
                for(var h=0; h<_scheduled_playlists[i][j].length; h++) {
                    let elem = _scheduled_playlists[i][j][h]
                    let loop_start = elem['start_iteration']
                    let loop_end = elem['end_iteration']
                    let loop_cycles = loop_end - loop_start
                    let loop_widget = elem['loop_widget']
                    let mode = elem['mode']
                    if (!loop_widget) {
                        root.logger.debug(`Loop widget ${elem.loop_id} not found, skipping`)
                        continue
                    }

                    var loop = loop_widget.maybe_loop
                    if (!loop) {
                        root.logger.debug(`Loop ${elem.loop_id} is not instantiated, creating a default back-end loop for it`)
                        loop_widget.create_backend_loop()
                        loop = loop_widget.maybe_loop
                        if (!loop) {
                            root.logger.warning(`Could not create a back-end loop for ${elem.loop_id}.`)
                            continue
                        }
                    }
                    // If our target is a CompositeLoop, it does not directly inherit a Python loop.
                    // Instead it will be stored in its py_loop subobject.
                    if (loop instanceof CompositeLoop) {
                        loop = loop.py_loop
                    }

                    if (!_schedule[loop_start]) { _schedule[loop_start] = { loops_start: new Set(), loops_end: new Set(), loops_ignored: new Set(), loop_modes: {} } }
                    if (!_schedule[loop_end]) { _schedule[loop_end] = { loops_start: new Set(), loops_end: new Set(), loops_ignored: new Set(), loop_modes: {} } }

                    if (loop_cycles > 0) {
                        _schedule[loop_start].loops_start.add(loop)
                        _schedule[loop_start].loop_modes[loop] = mode
                        _schedule[loop_end].loops_end.add(loop)
                    } else {
                        _schedule[loop_start].loops_ignored.add(loop)
                        _schedule[loop_start].loop_modes[loop] = mode
                    }
                }
            }
        }

        // For any loops that repeatedly play, just remove intermediate start and end entries.
        Object.keys(_schedule).map(k => {
            for (var starting of _schedule[k].loops_start) {
                if (_schedule[k].loops_end.has(starting)) {
                    _schedule[k].loops_end.delete(starting)
                    _schedule[k].loops_start.delete(starting)
                }
            }
            // Also convert the sets to lists for Python usage
            _schedule[k].loops_start = Array.from(_schedule[k].loops_start)
            _schedule[k].loops_end = Array.from(_schedule[k].loops_end)
            _schedule[k].loops_ignored = Array.from(_schedule[k].loops_ignored)
        })

        // Transform the schedule: move modes into loop_starts.
        for (var k in _schedule) {
            let v = _schedule[k]
            let modes = v.loop_modes
            let starts = v.loops_start
            delete v.loop_modes
            delete v.loops_start
            v.loops_start = starts.map(l => [l, modes[l]])
        }

        root.logger.trace(() => `full schedule:\n${
            Array.from(Object.entries(_schedule)).map(([k,v]) => 
                `- ${k}: stop [${Array.from(v.loops_end).map(l => l.obj_id)}], start [${Array.from(v.loops_start).map(l => l[0].obj_id + ` @ mode ${l[1]}`)}], ignore [${Array.from(v.loops_ignored).map(l => l.obj_id)}]`
            ).join("\n")
        }`)

        schedule = _schedule
    }
    function ensure_script_or_regular() {
        // We either want all modes specified or none (script or regular composite loop, resp.)
        // If some modes are specified and others aren't, we should fix that.
        var found_mode = null
        var modes_count = 0
        var all_count = 0
        playlists.forEach(p => p.forEach(pp => pp.forEach(e => {
            all_count += 1
            if(e.mode !== null && e.mode !== undefined) {
                found_mode = e.mode
                modes_count += 1
            }
        })))
        root.kind = (found_mode === null) ? 'regular' : 'script'
        if (found_mode !== null && modes_count < all_count) {
            let new_playlists = playlists.map(p => p.map(pp => pp.map(e => {
                var rval =  Object.create(e)
                let keys = Object.keys(e)
                for(var i=0; i<keys.length; i++) {
                    rval[keys[i]] = e[keys[i]]
                }
                rval.mode = found_mode
                return rval
            })))
            playlists_in = new_playlists
        }
    }
    function update_schedule() {
        if (!schedule_frozen) { recalculate_schedule() }
    }
    // During recording, we need to freeze the schedule. Otherwise, the changing lenghts of the recording sub-loop(s) will lead to a cyclic
    // change in the schedule while recording is ongoing.
    readonly property bool schedule_frozen: (ModeHelpers.is_recording_mode(mode) || (ModeHelpers.is_recording_mode(next_mode) && next_transition_delay === 0))
    readonly property bool sync_empty: !(sync_length > 0)
    onPlaylistsChanged: {
        root.logger.trace(() => `playlists -> ${JSON.stringify(playlists, null, 2)}`)
        ensure_script_or_regular();
        update_schedule()
    }
    onSync_emptyChanged: update_schedule()

    // Calculated properties
    readonly property int display_position : position

    onPositionChanged: root.logger.trace(() => `pos -> ${position}`)

    property var all_loop_ids: {
        var r = new Set()
        if (!playlists) { return [] }
        for(var i = 0; i < playlists.length; i++) {
            for (var j = 0; j < playlists[i].length; j++) {
                for (var h = 0; h < playlists[i][j].length; h++) {
                    r.add(playlists[i][j][h].loop_id)
                }
            }
        }
        return r
    }
    property var all_loops: new Set(Array.from(all_loop_ids || []).map(id => registries.objects_registry.value_or(id, undefined)).filter(m => m))
    property bool all_loops_found:  all_loops.size == all_loop_ids.size

    // If the registry changes and we didn't find all our loops, trigger to look again
    Connections {
        target: registries.objects_registry
        function onContentsChanged() { if (!all_loops_found) { root.all_loop_idsChanged() } }
    }

    function print_loops_found() { root.logger.debug(`All ${all_loop_ids.size} loops found: ${all_loops_found}`) }
    onAll_loops_foundChanged: print_loops_found()
    onAll_loop_idsChanged: print_loops_found()

    // If we didn't find all our loops, listen for registry changes to find them later
    Connections {
        target: all_loops_found ? null : registries.objects_registry
        function onItemAdded(id, val) { if (all_loop_ids.has(id)) { playlists_inChanged() } }
        function onItemModified(id, val) { if (all_loop_ids.has(id)) { playlists_inChanged() } }
    }

    // If we did find all our loops, listen to length changes to update the schedule
    Mapper {
        model: Array.from(all_loops)
        Connections {
            property var mapped_item
            property int index
            target: mapped_item
            function onN_cyclesChanged() { update_schedule() }
        }
    }

    property var sync_loop : registries.state_registry.sync_loop

    function transition(mode, maybe_delay, maybe_align_to_sync_at) {
        py_loop.transition(mode, maybe_delay, maybe_align_to_sync_at)
    }

    function actual_composition_descriptor() {
        return {
            'playlists': playlists
        }
    }

    function qml_close() {}

    function clear() {
        playlists_in = []
        playlists_inChanged()
    }

    function adopt_ringbuffers(reverse_start_cycle, cycles_length, go_to_cycle, go_to_mode) {
        py_loop.adopt_ringbuffers(reverse_start_cycle, cycles_length, go_to_cycle, go_to_mode)
    }
}