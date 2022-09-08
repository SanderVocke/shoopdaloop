import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import SLLooperManager 1.0
import '../LoopState.js' as LoopState

Item {
    anchors {
        fill: parent
        margins: 6
    }

    id: shared

    // TRACKS STATE
    property var track_names: [
        'Track 1',
        'Track 2',
        'Track 3',
        'Track 4',
        'Track 5',
        'Track 6',
        'Track 7',
        'Track 8',
    ]
    property int loops_per_track: 6
    property var loop_managers: {
        var outer, inner
        var managers = []
        for(outer = 0; outer < track_names.length; outer++) {
            var i_managers = []
            for(inner = 0; inner < loops_per_track; inner++) {
                var mgr = Qt.createQmlObject('import QtQuick 2.0; import SLLooperManager 1.0; SLLooperManager {sl_looper_index: ' + (outer * loops_per_track + inner).toString() + '}',
                                                   shared,
                                                   "dynamicSnippet1");
                mgr.connect_osc_link(osc_link)
                mgr.start_sync()
                i_managers.push(mgr)
            }
            managers.push(i_managers)
        }
        return managers
    }
    property var master_loop_idx: [0, 0]
    property var master_loop_manager: {
        return loop_managers[master_loop_idx[0]][master_loop_idx[1]]
    }
    property var selected_loops: [0, 0, 0, 0, 0, 0, 0, 0]

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

    // DERIVED STATE
    property var scene_names: []
    property var loops_of_selected_scene: []
    property var loops_of_hovered_scene: []

    // FUNCTIONS
    function actions_on_loop_mgrs_in_track(track_idx, loop_idx, on_idx_loop_fn, on_other_loop_fn) {
        for(var i = 0; i < loops_per_track; i++) {
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
        console.log('select ' + track_idx.toString() + ' ' + loop_idx.toString())
        // If we are playing another loop, tell SL to switch
        if(loop_idx >= 0 &&
           //track.selected_loop !== index &&
           [LoopState.LoopState.Muted, LoopState.LoopState.Off].includes(loop_managers[track_idx][loop_idx].state)) {
            actions_on_loop_mgrs_in_track(track_idx, loop_idx, (mgr) => { console.log('um') ;mgr.doUnmute() }, (mgr) => { console.log('m'); mgr.doMute() })
        }

        // Update everything else
        selected_loops[track_idx] = loop_idx
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
        for (var lidx in scenes[idx].loops) {
            var lp = scenes[idx].loops[lidx]
            select_loop(lp[0], lp[1])
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

    Connections {
        function onScenesChanged() { update_scene_names(); update_loops_of_selected_scene(); update_loops_of_hovered_scene() }
        function onSelected_sectionChanged() { update_selected_scene_from_section() }
        function onSelected_sceneChanged() { update_loops_of_selected_scene() }
        function onHovered_sceneChanged() { update_loops_of_hovered_scene() }
    }
}
