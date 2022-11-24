import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import '../../build/StatesAndActions.js' as StatesAndActions

// The track widget displays the state of a track (collection of
// loopers with shared settings/control).
Item {
    id: track
    property int num_loops
    property int first_index
    property int maybe_master_loop_idx: -1 //-1 is none
    property var master_loop_manager
    property var loop_managers
    property var port_manager
    property var loop_names
    property string name: ''
    property bool name_editable: true

    // Array of loop idxs
    property var loops_of_selected_scene: []
    property var loops_of_hovered_scene: []

    width: childrenRect.width
    height: childrenRect.height

    signal set_loop_in_scene(int idx)
    signal renamed(string name)
    signal request_select_loop(int idx)
    signal request_rename_loop(int idx, string name)
    signal request_clear_loop(int idx)

    function actions_on_loop_mgrs(idx, on_idx_loop_fn, on_other_loop_fn) {
        for(var i = 0; i < track.num_loops; i++) {
            var mgr = loop_managers[i]
            if (idx === i) {
                on_idx_loop_fn(mgr)
            }
            else {
                on_other_loop_fn(mgr)
            }
        }
    }

    Rectangle {
        property int x_spacing: 8
        property int y_spacing: 4

        width: childrenRect.width + x_spacing
        height: childrenRect.height + y_spacing
        color: "#555555"

        Item {
            width: childrenRect.width
            height: childrenRect.height
            x: parent.x_spacing/2
            y: parent.y_spacing/2

            Column {
                spacing: 2

                TextField {
                    text: track.name
                    width: 90
                    font.pixelSize: 13
                    readOnly: !track.name_editable

                    onEditingFinished: () => {
                                           background_focus.forceActiveFocus()
                                           track.renamed(text)
                                       }
                }

                Repeater {
                    model: track.num_loops
                    id: loops
                    width: childrenRect.width
                    height: childrenRect.height

                    LoopWidget {
                        id: lwidget

                        is_master: track.maybe_master_loop_idx === index
                        is_in_selected_scene: track.loops_of_selected_scene.includes(index)
                        is_in_hovered_scene: track.loops_of_hovered_scene.includes(index)
                        manager: track.loop_managers[index]
                        name: track.loop_names[index]
                        internal_name: track.name + ' loop ' + (index+1).toString()

                        onAdd_to_scene: () => { track.set_loop_in_scene(index) }
                        onRequest_rename: (name) => { track.request_rename_loop(index, name) }
                        onRequest_clear: () => { track.request_clear_loop(index) }
                    }
                }

                TrackControlWidget {
                    id: trackctlwidget

                    muted: track.active_loop_state === StatesAndActions.LoopState.Muted
                    port_manager: track.port_manager
                }
            }
        }
    }
}
