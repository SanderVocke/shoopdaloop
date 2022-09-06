import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

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

    // SCENES STATE
    property var scenes: [
        // { name: "scene_name", loops: [[0, 0], [1,4]] }
    ]
    property int selected_scene: -1
    property int hovered_scene: -1

    // SEQUENCING STATE
    property var sections: [
        { name: 'Section 1', scene_idx: -1 },
        { name: 'Section 2', scene_idx: -1 }
    ]
    property int selected_section: -1

    // DERIVED STATE
    property var scene_names: []
    property var loops_of_selected_scene: []
    property var loops_of_hovered_scene: []

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
