import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import BackendLooperManager 1.0
import '../../build/LoopState.js' as LoopState

Item {
    anchors {
        fill: parent
        margins: 6
    }

    id: shared

    // STATIC
    property int loops_per_track: 6
    property int tracks: 8
    property var loop_managers: {
        // Nested array of loop mgrs per track. Master loop is regarded as the first track.

        function mgr_snippet (dry_loop_idx, wet_loop_idx, sync) {
            return 'import QtQuick 2.0\n' +
            'import BackendFXLooperPairManager 1.0\n' +
            'BackendFXLooperPairManager { \n' +
                'dry_looper_idx: ' + dry_loop_idx.toString() + '\n' +
                'wet_looper_idx: ' + wet_loop_idx.toString() + '\n' +
                ' }';
        }

        var outer, inner
        var managers = []
        var master_mgr = Qt.createQmlObject(mgr_snippet(0, 2, true),
                            shared,
                            "dynamicSnippet1");
        master_mgr.connect_midi_control_manager(midi_control_manager, 0, 0)
        master_mgr.connect_backend_manager(backend_manager)
        managers.push([master_mgr])
        for(outer = 0; outer < tracks; outer++) {
            var i_managers = []
            for(inner = 0; inner < loops_per_track; inner++) {
                var mgr = Qt.createQmlObject(mgr_snippet(
                    (outer * loops_per_track + inner)*4 + 4,
                    (outer * loops_per_track + inner)*4 + 6,
                    true),
                    shared,
                    "dynamicSnippet1");
                mgr.connect_midi_control_manager(midi_control_manager, outer + 1, inner)
                mgr.connect_backend_manager(backend_manager)
                i_managers.push(mgr)
            }
            managers.push(i_managers)
        }
        return managers
    }
    property var master_loop_manager: {
        return loop_managers[0][0];
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
    property var selected_loops: Array(tracks+1).fill(0)
    property var loop_names: {
        var outer, inner
        var names = []
        names.push(['Master'])
        for (outer = 0; outer < tracks; outer++) {
            var track_loop_names = []
            for (inner = 0; inner < loops_per_track; inner++) {
                track_loop_names.push('')
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

    // SEQUENCING STATE
    property var sections: [
        { name: 'Section 1', scene_idx: -1, actions: []},
        { name: 'Section 2', scene_idx: -1, actions: []}
    ]
    property int selected_section: -1

    // (DE-)SERIALIZATION
    function state_to_dict() {
        return {
            'track_names': track_names,
            'selected_loops': selected_loops,
            'scenes': scenes,
            'selected_scene': selected_scene,
            'hovered_scene': hovered_scene,
            'sections': sections,
            'selected_section': selected_section,
            'loop_names': loop_names
        }
    }
    function set_state_from_dict(state_dict) {
        // RESET SELECTIONS
        selected_loops = [0, 0, 0, 0, 0, 0, 0, 0, 0]
        selected_scene = -1
        hovered_scene = -1
        selected_section = -1

        selected_loopsChanged()
        selected_sceneChanged()
        hovered_sceneChanged()
        selected_sectionChanged()

        // LOAD
        track_names = state_dict.track_names
        scenes = state_dict.scenes
        sections = state_dict.sections
        loop_names = state_dict.loop_names
        selected_loops = state_dict.selected_loops
        selected_scene = state_dict.selected_scene
        hovered_scene = state_dict.hovered_scene
        selected_section = state_dict.selected_section

        track_namesChanged()
        loop_namesChanged()
        scenesChanged()
        sectionsChanged()
        selected_loopsChanged()
        selected_sceneChanged()
        hovered_sceneChanged()
        selected_sectionChanged()
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

    function set_track_pan(track_idx, pan) {
        for(var i = 0; i < loop_managers[track_idx].length; i++) {
            loop_managers[track_idx][i].panL = Math.max(pan, 0.0)
            loop_managers[track_idx][i].panR = Math.min(pan, 0.0) + 1.0
        }
    }

    function set_track_volume(track_idx, vol) {
        // TODO implement
    }

    function set_track_passthrough(track_idx, level) {
        // TODO implement
    }

    function select_loop(track_idx, loop_idx) {
        if (selected_loops[track_idx] == loop_idx) { return; }
        

        // TODO make any state changes to loops
        

        // Update everything else
        selected_loops[track_idx] = loop_idx
        selected_loopsChanged()
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

    function activate_scene(idx) {
        var tracks_covered = []
        // Activate all loops listed in the scene
        for (var lidx in scenes[idx].loops) {
            var lp = scenes[idx].loops[lidx]
            select_loop(lp[0], lp[1])
            tracks_covered.push(parseInt(lp[0]))
        }
        // Deselect loops in any track that has no
        // loop in this scene
        for (var tidx in track_names) {
            if (!tracks_covered.includes(parseInt(tidx))) {
                select_loop(tidx, -1)
            }
        }
    }

    function select_section(idx) {
        selected_section = idx
        selected_sectionChanged()
    }

    function rename_section(idx, name) {
        sections[idx].name = name
        sectionsChanged()
    }

    function change_section_scene(section_idx, scene_idx) {
        sections[section_idx].scene_idx = scene_idx
        sectionsChanged()
    }

    function bind_loop_to_current_scene(track_idx, loop_idx) {
        if (selected_scene >= 0) {
            // remove any previous setting for this track
            var modified = scenes[selected_scene].loops
            modified = modified.filter(l => l[0] !== track_idx)
            // add the new setting
            modified.push([track_idx, loop_idx])
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

    function add_action(section, type, track) {
        var idx
        var found = false
        for (idx in sections[section].actions) {
            var act = sections[section].actions[idx]
            if (act[0] === type && act[1] === track) {
                found = true
                break
            }
        }
        if (!found) {
            sections[section].actions.push([type, track])
            sectionsChanged()
        }
    }

    function remove_action(section, type, track) {
        var idx
        var found_idx = -1
        for (idx in sections[section].actions) {
            var act = sections[section].actions[idx]
            if (act[0] === type && act[1] === track) {
                found_idx = idx
                break
            }
        }
        if (found_idx >= 0) {
            sections[section].actions.splice(found_idx, 1)
            sectionsChanged()
        }
    }

    function update_scene_names() {
        var names = []
        var idx
        for (idx in scenes) { names.push(scenes[idx].name) }
        scene_names = names
        scene_namesChanged()
    }

    function update_selected_scene_from_section() {
        if (selected_section >= 0) {
            selected_scene = sections[selected_section].scene_idx
            selected_sceneChanged()
        }
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

    function load_loop_wav(track_idx, loop_idx, wav_file) {
        var mgr = loop_managers[track_idx][loop_idx];
        // TODO implement: mgr.doLoadWav(wav_file);
    }

    function save_loop_wav(track_idx, loop_idx, wav_file) {
        var mgr = loop_managers[track_idx][loop_idx];
        // TODO implement: mgr.doSaveWav(wav_file);
    }

    function clear_loop(track_idx, loop_idx) {
        var mgr = loop_managers[track_idx][loop_idx];
        // TODO: implement mgr.doClear();
    }

    function save_session(filename, store_audio) {
        var my_state = serialize_state()
        // sl_global_manager.save_session(my_state, [].concat(...loop_managers), store_audio, filename)
    }

    function load_session(filename) {
        // var my_state = sl_global_manager.load_session([].concat(...loop_managers), osc_link, filename)
        deserialize_state(my_state)
    }

    Connections {
        function onScenesChanged() { update_scene_names(); update_loops_of_selected_scene(); update_loops_of_hovered_scene() }
        function onSelected_sectionChanged() {
            update_selected_scene_from_section()
            midi_control_manager.active_scripting_section_changed(selected_section)
        }
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
            set_track_pan(track, value)
        }
        function onSetVolume(track, value) {
            console.log(track, value, 'yeah')
            if (track < 0 || track >= loop_managers.length) {
                console.log("Ignoring MIDI control for out-of-bounds track: " + track.toString())
                return
            }
            set_track_volume(track, value)
        }
        function onLoopAction(track, loop, action, args) {
            loop_idx =
                track == 0 ?
                    loop :
                    (track-1)*loops_per_track + loop_managers[0].length + loop;
            backend_manager.do_loop_action(loop_idx, action, args);
        }
    }
}
