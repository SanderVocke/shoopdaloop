import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import '../../build/types.js' as Types

// The track widget displays the state of a track (collection of
// loopers with shared settings/control).
Item {
    id: track

    property int num_slots

    property var master_loop   // LoopWidget
    property var targeted_loop // LoopWidget

    //FIXME: property var ports_manager

    property string name: ''
    property bool name_editable: true

    // Array of LoopWidget
    property var loops_of_selected_scene: []
    property var loops_of_hovered_scene:  []

    width: childrenRect.width
    height: childrenRect.height

    signal toggle_loop_in_scene(var loop)
    signal renamed(string name)
    signal request_select_loop(var loop)
    signal request_rename_loop(var loop, string name)
    signal request_clear_loop(var loop)
    signal request_toggle_loop_selected(var loop)
    signal request_set_targeted_loop(var loop)

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
                        master_manager : track.master_loop_manager
                        targeted_loop_manager: track.targeted_loop_manager
                        ports_manager: track.ports_manager
                        name: track.loop_names[index]
                        internal_name: track.name + ' loop ' + (index+1).toString()

                        onToggle_in_current_scene: () => { track.toggle_loop_in_scene(index) }
                        onRequest_rename: (name) => { track.request_rename_loop(index, name) }
                        onRequest_clear: () => { track.request_clear_loop(index) }
                        onRequest_toggle_selected: () => { track.request_toggle_loop_selected(index) }
                        onRequest_set_as_targeted: () => { track.request_set_targeted_loop(index) }
                    }
                }

                Item {
                    height: 5
                    width: 10
                }

                TrackControlWidget {
                    id: trackctlwidget

                    muted: track.active_loop_state === StatesAndActions.LoopMode.Muted
                    ports_manager: track.ports_manager
                }
            }
        }
    }
}
