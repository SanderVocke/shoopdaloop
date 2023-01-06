import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import NChannelAbstractLooperManager 1.0
import '../../build/StatesAndActions.js' as StatesAndActions

Item {
    anchors {
        fill: parent
        margins: 6
    }

    id: shared

    // STATIC
    property int loops_per_track: 8
    property int tracks: 8
    property var loop_managers: {
        // Nested array of logical loop mgrs per track. Master loop is regarded as the first track.
        var outer, inner
        var managers = []
        var master_mgr = backend_manager.logical_looper_managers[0]
        managers.push([master_mgr])
        var stop_others_closure = (track, loop) => {
            return () => {
                    actions_on_loop_mgrs_in_track(
                        track, loop, () => {},
                        (m) => m.doLoopAction(StatesAndActions.LoopActionType.DoStop, [0.0], true)
                    )
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
                i_managers.push(mgr)
            }
            managers.push(i_managers)
        }
        return managers
    }
    property var master_loop_manager: {
        return loop_managers[0][0];
    }
    property var port_managers: {
        var managers = []
        for (var i=0; i<tracks+1; i++) {
            var mgr = backend_manager.track_port_managers[i]
            managers.push(mgr)
        }
        return managers
    }

    // TRACKS STATE
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

    // SCENES STATE
    property var scenes: [
        // { name: "scene_name", loops: [[0, 0], [1,4]] }
    ]
    property int selected_scene: -1
    property int hovered_scene: -1

    // SCRIPTING STATE
    property var sections: [
        { name: 'Test Sect', actions: [
            { 'action_type': 'loop', 'track': 1, 'loop': 0, 'action': 'record', 'stop_others': 'no', 'on_cycle': 1 },
            { 'action_type': 'loop', 'track': 1, 'loop': 1, 'action': 'record', 'stop_others': 'no', 'on_cycle': 1 },
            { 'action_type': 'loop', 'track': 1, 'loop': 1, 'action': 'play',   'stop_others': 'no', 'on_cycle': 2 },
            { 'action_type': 'loop', 'track': 1, 'loop': 0, 'action': 'play',   'stop_others': 'no', 'on_cycle': 3 }
        ], duration: 4 }
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
                if (loop.state != StatesAndActions.LoopState.Stopped &&
                    loop.state != StatesAndActions.LoopState.Empty) {
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
        master_loop_manager.doLoopAction(StatesAndActions.LoopActionType.DoPlay, [0.0], false)
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
        console.log(selected_scene)
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
                    loop_managers[track][loop].doLoopAction(StatesAndActions.LoopActionType.DoPlay, [0.0], true)
                } else {
                    loop_managers[track][loop].doLoopAction(StatesAndActions.LoopActionType.DoStop, [0.0], true)
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
                loop_managers[track+1][loop].doLoopAction(StatesAndActions.LoopActionType.DoStop, [0.0], true)
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
                    loop_managers[track][idx].doLoopAction(StatesAndActions.LoopActionType.DoStop, [0.0], true)
                }
            }
        }
        var stop_others = (track, loop) => {
            // Note: skip master track, this should always run
            for(var trk=1; trk<loop_managers.length; trk++) {
                for(var idx=0; idx<loop_managers[trk].length; idx++) {
                    if(idx != loop || trk != track) {
                        loop_managers[trk][idx].doLoopAction(StatesAndActions.LoopActionType.DoStop, [0.0], true)
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
                    loop_managers[action['track']][action['loop']].doLoopAction(StatesAndActions.LoopActionType.DoPlay, [0.0], true)
                    break;
                case 'record':
                    if(action['stop_others'] == 'track') { stop_others_in_track(action['track'], action['loop']) }
                    else if(action['stop_others'] == 'all') { stop_others(action['track'], action['loop']) }
                    loop_managers[action['track']][action['loop']].doLoopAction(StatesAndActions.LoopActionType.DoRecord, [0.0], true)
                    break;
                case 'stop':
                    loop_managers[action['track']][action['loop']].doLoopAction(StatesAndActions.LoopActionType.DoStop, [0.0], true)
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
    }

    Connections {
        target: midi_control_manager
        function onSetPan(track, value) {
            if (track < 0 || track >= loop_managers.length) {
                console.log("Ignoring MIDI control for out-of-bounds track: " + track.toString())
                return
            }
            // TODO port_managers[track].pan = value
        }
        function onSetVolume(track, value) {
            if (track < 0 || track >= loop_managers.length) {
                console.log("Ignoring MIDI control for out-of-bounds track: " + track.toString())
                return
            }
            port_managers[track].volume = value
        }
        function onLoopAction(track, loop, action, args) {
            loop_idx =
                track == 0 ?
                    loop :
                    (track-1)*loops_per_track + loop_managers[0].length + loop;
            backend_manager.do_loop_action(loop_idx, action, args);
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
