import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import NChannelAbstractLooperManager 1.0
import '../../build/StatesAndActions.js' as StatesAndActions
import '../state_helpers.js' as State_helpers

Item {
    anchors {
        fill: parent
        margins: 6
    }

    id: shared

    // STATIC
    readonly property int loops_per_track: 8
    readonly property int tracks: 8
    readonly property var loop_managers: {
        // Nested array of logical loop mgrs per track. Master loop is regarded as the first track.
        var outer, inner
        var managers = []
        var master_mgr = backend_manager.logical_looper_managers[0]
        managers.push([master_mgr])
        var stop_others_closure = (track, loop) => {
            return () => {
                    actions_on_loop_mgrs_in_track(
                        track, loop, () => {},
                        (m) => m.doLoopAction(StatesAndActions.LoopActionType.DoStop, [0.0], true, false)
                    )
                }
        }
        var state_changed_closure = (track, loop, mgr) => {
            return () =>
                midi_control_manager.loop_state_changed(
                    track,
                    loop,
                    mgr.mode,
                    mgr.selected,
                    mgr.targeted)
        }
        var did_loop_action_closure = (track, loop) => {
            return (action, args, with_soft_sync, propagate_to_selected_loops) => {
                if (propagate_to_selected_loops) {
                    shared.propagate_action_to_selected_loops(track, loop, action, args, with_soft_sync)
                }
            }
        }
        for(outer = 0; outer < tracks; outer++) {
            var i_managers = []
            for(inner = 0; inner < loops_per_track; inner++) {
                var track = outer+1
                var loop = inner
                var mgr = backend_manager.logical_looper_managers[1 + outer * loops_per_track + inner]
                // Solo playing in track is handled here
                mgr.stopOtherLoopsInTrack.connect(stop_others_closure(track, loop))
                mgr.modeChanged.connect(state_changed_closure(track, loop, mgr))
                mgr.selectedChanged.connect(state_changed_closure(track, loop, mgr))
                mgr.targetedChanged.connect(state_changed_closure(track, loop, mgr))
                mgr.didLoopAction.connect(did_loop_action_closure(track, loop))
                i_managers.push(mgr)
            }
            managers.push(i_managers)
        }
        return managers
    }
    readonly property var master_loop_manager: {
        return loop_managers[0][0];
    }
    readonly property var port_managers: {
        var managers = []
        for (var i=0; i<tracks+1; i++) {
            var mgr = backend_manager.track_port_managers[i]
            managers.push(mgr)
        }
        return managers
    }
    readonly property var targeted_loop_manager: targeted_loop !== undefined ? loop_managers[targeted_loop[0]][targeted_loop[1]] : undefined

    // TRACKS mode
    property var track_names: {
        var r = []
        r.push('Master')
        for (var i = 0; i < tracks; i++) {
            r.push('Track ' + (i+1).toString())
        }
        return r
    }
    property var loop_names: {
        var outer, inner
        var names = []
        names.push(['Master'])
        for (outer = 0; outer < tracks; outer++) {
            var track_loop_names = []
            for (inner = 0; inner < loops_per_track; inner++) {
                track_loop_names.push('(' + (outer+1).toString() + ', ' + (inner+1).toString() + ')')
            }
            names.push(track_loop_names)
        }
        return names
    }
    // Loop selection is useful for doing an action on a group of loops,
    // or for MIDI controllers to select the loop(s) to operate on.
    property var selected_loops: [] // list of [track_idx, loop_idx]

    // The "targeted" loop is a special selection which contains only
    // one loop. It is used for operations on loops w.r.t. another.
    property var targeted_loop: undefined

    // SCENES STATE
    property var scenes: [
        // { name: "scene_name", loops: [[0, 0], [1,4]] }
    ]
    property int selected_scene: -1
    property int hovered_scene: -1

    // SCRIPTING STATE
    property var sections: [
        //{ name: 'Test Sect', actions: [
        //    { 'action_type': 'loop', 'track': 1, 'loop': 0, 'action': 'record', 'stop_others': 'no', 'on_cycle': 1 },
        //], duration: 4 }
    ]
    // Playback control is based on the active cycle, meaning
    // how many times the master loop has cycled.
    property int script_current_cycle: 0
    property bool script_playing: false
    property bool script_pause_after_sections: false

    // (DE-)SERIALIZATION
    function state_to_dict() {
        return {
            'track_names': track_names,
            'loop_names': loop_names,
            'scenes': scenes,
            'sections': sections,
        }
    }
    function set_state_from_dict(state_dict) {
        // RESET SELECTIONS
        selected_scene = -1
        hovered_scene = -1

        selected_sceneChanged()
        hovered_sceneChanged()

        // LOAD
        track_names = state_dict.track_names
        scenes = state_dict.scenes
        sections = state_dict.sections
        loop_names = state_dict.loop_names

        track_namesChanged()
        loop_namesChanged()
        scenesChanged()
        sectionsChanged()
    }
    function serialize_state() {
        return JSON.stringify(state_to_dict())
    }
    function deserialize_state(data) {
        set_state_from_dict(JSON.parse(data))
    }

    // DERIVED STATE
    property var scene_names: []
    property var loops_of_selected_scene: []
    property var loops_of_hovered_scene: []

    readonly property int script_total_cycles: {
        var r = 0
        for (const s of sections) {
            r += s.duration
        }
        return r
    }

    // SCRIPTING DERIVED STATE
    function active_section_and_offset(cycle) {
        for (var i = 0; i<sections.length; i++) {
            var offset = cycle - section_start_cycles[i]
            if (offset >= 0 && offset < sections[i].duration) {
                return [i, offset]
            }
        }
        return [-1, -1];
    }
    readonly property var section_start_cycles: {
        var tot = 0
        var r = []
        for (var section of sections) {
            r.push (tot)
            tot += section.duration
        }
        return r
    }
    readonly property int current_section_idx: active_section_and_offset(script_current_cycle)[0]
    readonly property int current_section_cycle_offset: active_section_and_offset(script_current_cycle)[1]
    readonly property bool pause_at_next_cycle: {
        return !script_playing ||
               (script_pause_after_sections && 
                active_section_and_offset(script_current_cycle)[0] != active_section_and_offset(script_current_cycle+1)[0]);
    }
    readonly property int script_length: {
        var l = 0;
        for(var sect of sections) { l += sect.duration }
        return l
    }

    // SCRIPTING BEHAVIOR AND SIGNALING
    signal action_executed (int section_idx, int action_idx)
    function all_stopped() {
        for(var trk=0; trk<loop_managers.length; trk++) {
            for(var loop of loop_managers[trk]) {
                if (loop.mode != StatesAndActions.LoopMode.Stopped &&
                    loop.mode != StatesAndActions.LoopMode.Empty) {
                        return false
                    }
            }
        }
        return true
    }
    function play_script() {
        if (!all_stopped()) {
            console.log('TODO throw error: not all stopped')
            return
        }
        execute_actions(script_current_cycle)
        script_playing = true
        master_loop_manager.doLoopAction(StatesAndActions.LoopActionType.DoPlay, [0.0], false, false)
    }
    function stop_script() { script_playing = false }
    function execute_actions(script_cycle) {
        var sect_and_offset = active_section_and_offset(script_cycle)
        if(sect_and_offset[0] >= 0) {
            var section = sections[sect_and_offset[0]]
            var offset = sect_and_offset[1]
            for(var i=0; i<section['actions'].length; i++) {
                var action = section['actions'][i]
                if (offset == action['on_cycle']) {
                    execute_action(action);
                    action_executed(sect_and_offset[0], i)
                }
            }
        }
    }

    Connections {
        target: master_loop_manager
        function onCycled() {
            if(script_playing) {
                script_current_cycle += 1
                if(pause_at_next_cycle) { stop_script() }
            }
        }
        function onPassed_halfway() {
            if(script_playing && !pause_at_next_cycle) {
                execute_actions(script_current_cycle+1)
            }
        }
    }

    // FUNCTIONS
    function actions_on_loop_mgrs_in_track(track_idx, loop_idx, on_idx_loop_fn, on_other_loop_fn) {
        for(var i = 0; i < loop_managers[track_idx].length; i++) {
            var mgr = loop_managers[track_idx][i]
            if (loop_idx === i) {
                on_idx_loop_fn(mgr)
            }
            else {
                on_other_loop_fn(mgr)
            }
        }
    }

    function select_loop(track_idx, loop_idx) {
        selected_loops = selected_loops.filter(e => !(e[0] == track_idx && e[1] == loop_idx)).concat([[track_idx, loop_idx]])
        selected_loopsChanged()
    }

    function deselect_loop(track_idx, loop_idx) {
        selected_loops = selected_loops.filter(e => !(e[0] == track_idx && e[1] == loop_idx))
        selected_loopsChanged()
    }

    function toggle_loop_selected(track_idx, loop_idx) {
        if (targeted_loop !== undefined &&
            targeted_loop[0] == track_idx &&
            targeted_loop[1] == loop_idx) {
            targeted_loop = undefined
        } else if (loop_managers[track_idx][loop_idx].selected) {
            deselect_loop(track_idx, loop_idx)
        } else {
            select_loop(track_idx, loop_idx)
        }
    }

    function select_targeted_loop(track_idx, loop_idx) {
        deselect_loop(track_idx, loop_idx)
        targeted_loop = [track_idx, loop_idx]
    }

    function deselect_targeted_loop() {
        targeted_loop = undefined
    }

    function propagate_action_to_selected_loops(track, loop, action, args, with_soft_sync) {
        for (var l of selected_loops) {
            if (!(track == l[0] && loop == l[1])) {
                loop_managers[l[0]][l[1]].doLoopAction(action, args, with_soft_sync, false)
            }
        }
    }

    function rename_scene(idx, name) {
        scenes[idx].name = name
        scenesChanged()
    }

    function add_scene() {
        var name_idx = 1
        var candidate
        while (true) {
            candidate = 'scene ' + name_idx.toString()
            name_idx = name_idx + 1
            var found = false
            var idx
            for (idx in scenes) { if(scenes[idx].name === candidate) { found = true; break; } }
            if (!found) { break }
        }

        scenes.push({ name: candidate, loops: [] })
        scenesChanged()
    }

    function remove_scene(idx) {
        scenes.splice(idx, 1)
        if (hovered_scene > idx) {
            hovered_scene = hovered_scene - 1
            hovered_sceneChanged()
        }
        else if(hovered_scene === idx) {
            hovered_scene = -1
            hovered_sceneChanged()
        }
        selected_scene = -1
        selected_sceneChanged()
        scenesChanged()
    }

    function select_scene(idx) {
        selected_scene = idx
        selected_sceneChanged()
    }

    function hover_scene(idx) {
        hovered_scene = idx
        hovered_sceneChanged()
    }

    function play_scene(idx) {
        for (var track = 1; track <= tracks; track++) {
            for (var loop = 0; loop < loops_per_track; loop++) {
                if(scenes[idx].loops.find((elem) => elem[0] == track && elem[1] == loop) !== undefined) {
                    loop_managers[track][loop].doLoopAction(StatesAndActions.LoopActionType.DoPlay, [0.0], true, false)
                } else {
                    loop_managers[track][loop].doLoopAction(StatesAndActions.LoopActionType.DoStop, [0.0], true, false)
                }
            }
        }
    }

    function rename_section(idx, name) {
        sections[idx].name = name
        sectionsChanged()
    }

    function delete_section(idx) {
        sections.splice(idx, 1)
        sectionsChanged()
    }

    function add_section() {
        sections.push({ name: 'Section', actions: [], duration: 4})
        sectionsChanged()
    }

    function set_section_duration(section, duration) {
        sections[section].duration = duration
        sectionsChanged()
    }

    function toggle_loop_in_current_scene(track_idx, loop_idx) {
        if (selected_scene >= 0) {
            var modified = scenes[selected_scene].loops
            var idx = modified.findIndex((elem) => elem[0] == track_idx && elem[1] == loop_idx)
            if (idx == -1) {
                modified.push([track_idx, loop_idx])
            } else {
                modified.splice(idx, 1)
            }
            scenes[selected_scene].loops = modified
            scenesChanged()
        }
    }

    function rename_track(track_idx, name) {
        track_names[track_idx] = name
        track_namesChanged()
    }

    function rename_loop(track_idx, loop_idx, name) {
        loop_names[track_idx][loop_idx] = name
        loop_namesChanged()
    }

    function add_action(section, action) {
        sections[section].actions.push(action)
        sectionsChanged()
    }

    function remove_action(section, idx) {
        sections[section].actions.splice(idx, 1)
        sectionsChanged()
    }

    function update_scene_names() {
        var names = []
        var idx
        for (idx in scenes) { names.push(scenes[idx].name) }
        scene_names = names
        scene_namesChanged()
    }

    function update_loops_of_selected_scene() {
        if (selected_scene == -1) {
            loops_of_selected_scene = []
        } else {
            loops_of_selected_scene = scenes[selected_scene].loops
        }
        loops_of_selected_sceneChanged()
    }

    function update_loops_of_hovered_scene() {
        if (hovered_scene == -1) {
            loops_of_hovered_scene = []
        } else {
            loops_of_hovered_scene = scenes[hovered_scene].loops
        }
        loops_of_hovered_sceneChanged()
    }

    function clear_loop(track_idx, loop_idx) {
        var mgr = loop_managers[track_idx][loop_idx];
        // TODO: implement mgr.doClear();
    }

    function stop_all_except_master() {
        for (var track = 0; track < tracks; track++) {
            for (var loop = 0; loop < loops_per_track; loop++) {
                loop_managers[track+1][loop].doLoopAction(StatesAndActions.LoopActionType.DoStop, [0.0], true, false)
            }
        }
    }

    function save_session(filename, store_audio) {
        var my_state = serialize_state()
        backend_manager.save_session(filename, my_state, store_audio)
    }

    function load_session(filename) {
        backend_manager.load_session(filename)
    }

    function execute_action(action) {
        var stop_others_in_track = (track, loop) => {
            for(var idx=0; idx<loop_managers[track].length; idx++) {
                if(idx != loop) {
                    loop_managers[track][idx].doLoopAction(StatesAndActions.LoopActionType.DoStop, [0.0], true, false)
                }
            }
        }
        var stop_others = (track, loop) => {
            // Note: skip master track, this should always run
            for(var trk=1; trk<loop_managers.length; trk++) {
                for(var idx=0; idx<loop_managers[trk].length; idx++) {
                    if(idx != loop || trk != track) {
                        loop_managers[trk][idx].doLoopAction(StatesAndActions.LoopActionType.DoStop, [0.0], true, false)
                    }
                }
            }
        }

        switch (action['action_type']) {
        case 'loop':
            switch (action['action']) {
                case 'play':
                    if(action['stop_others'] == 'track') { stop_others_in_track(action['track'], action['loop']) }
                    else if(action['stop_others'] == 'all') { stop_others(action['track'], action['loop']) }
                    loop_managers[action['track']][action['loop']].doLoopAction(StatesAndActions.LoopActionType.DoPlay, [0.0], true, false)
                    break;
                case 'record':
                    if(action['stop_others'] == 'track') { stop_others_in_track(action['track'], action['loop']) }
                    else if(action['stop_others'] == 'all') { stop_others(action['track'], action['loop']) }
                    loop_managers[action['track']][action['loop']].doLoopAction(StatesAndActions.LoopActionType.DoRecord, [0.0], true, false)
                    break;
                case 'stop':
                    loop_managers[action['track']][action['loop']].doLoopAction(StatesAndActions.LoopActionType.DoStop, [0.0], true, false)
                    break;
            }
            break
        case 'scene':
            switch (action['action']) {
                case 'play':
                    console.log('unimplemented')
                    break
            }
            break
        }
    }

    Connections {
        function onScenesChanged() { update_scene_names(); update_loops_of_selected_scene(); update_loops_of_hovered_scene() }
        function onSelected_sceneChanged() {
            update_loops_of_selected_scene()
            midi_control_manager.active_scene_changed(selected_scene)
        }
        function onHovered_sceneChanged() { update_loops_of_hovered_scene() }
        function onSelected_loopsChanged() {
            for (var track=0; track < loop_managers.length; track++) {
                for (var loop=0; loop < loop_managers[track].length; loop++) {
                    loop_managers[track][loop].selected = (selected_loops.find(e => e[0] == track && e[1] == loop) !== undefined)
                }
            }
        }
        function onTargeted_loopChanged() {
            for (var track=0; track < loop_managers.length; track++) {
                for (var loop=0; loop < loop_managers[track].length; loop++) {
                    loop_managers[track][loop].targeted = (targeted_loop !== undefined && track == targeted_loop[0] && loop == targeted_loop[1])
                }
            }
            midi_control_manager.targeted_loop = targeted_loop !== undefined ?
                loop_managers[targeted_loop[0]][targeted_loop[1]] : undefined
        }
    }

    Connections {
        target: midi_control_manager
        function onSetVolume(track, value) {
            if (track < 0 || track >= loop_managers.length) {
                console.log("Ignoring MIDI control for out-of-bounds track: " + track.toString())
                return
            }
            port_managers[track].volume = value
        }
        function onLoopAction(track, loop, action, args) {
            if (action == StatesAndActions.LoopActionType.DoSelect) {
                toggle_loop_selected(track, loop)
            } else if (action == StatesAndActions.LoopActionType.DoTarget) {
                select_targeted_loop(track, loop)
            } else if (action == StatesAndActions.LoopActionType.DoRecord && targeted_loop_manager !== undefined) {
                // A target loop is set. Do the "record together with" functionality.
                // TODO: code is duplicated in app shared mode for GUI loop widget
                var n_cycles_delay = 0
                var n_cycles_record = 1
                n_cycles_record = Math.ceil(targeted_loop_manager.length / master_loop_manager.length)
                if (State_helpers.is_playing_state(targeted_loop_manager.mode)) {
                    var current_cycle = Math.floor(targeted_loop_manager.pos / master_loop_manager.length)
                    n_cycles_delay = Math.max(0, n_cycles_record - current_cycle - 1)
                }
                loop_managers[track][loop].doLoopAction(StatesAndActions.LoopActionType.DoRecordNCycles,
                                                        [n_cycles_delay, n_cycles_record],
                                                        true, true);
            } else {
                loop_managers[track][loop].doLoopAction(action, args, true, true)
            }
        }
    }

    Connections {
        target: backend_manager
        function onNewSessionStateStr(state_str) {
            deserialize_state(state_str)
        }
        function onRequestLoadSession(filename) {
            load_session(filename)
        }
        function onRequestSaveSession(filename) {
            save_session(filename, true)
        }
    }
}
