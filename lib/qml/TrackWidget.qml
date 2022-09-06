import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import '../LoopState.js' as LoopState

// The track widget displays the state of a track (collection of
// loopers with shared settings/control).
Item {
    id: track
    property int num_loops
    property int first_index
    property int track_index
    property int selected_loop
    property int maybe_master_loop: -1 //-1 is none

    // Array of loop idxs
    property var loops_of_selected_scene: []
    property var loops_of_hovered_scene: []

    width: childrenRect.width
    height: childrenRect.height

    signal set_loop_in_scene(int idx)

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
                    placeholderText: qsTr("Track " + track.track_index.toString())
                    width: 90
                    font.pixelSize: 15

                    onEditingFinished: background_focus.forceActiveFocus()
                }

                Repeater {
                    model: track.num_loops
                    id: loops
                    width: childrenRect.width
                    height: childrenRect.height

                    LoopWidget {
                        loop_idx: track.first_index + index
                        osc_link_obj: osc_link
                        is_selected: track.selected_loop === index
                        is_master: track.maybe_master_loop === index
                        is_in_selected_scene: track.loops_of_selected_scene.includes(index)
                        is_in_hovered_scene: track.loops_of_hovered_scene.includes(index)

                        onSelected: track.selected_loop = index
                        onAdd_to_scene: () => { track.set_loop_in_scene(index) }
                    }
                }

                TrackControlWidget {
                    id: trackctlwidget

                    function active_loop_state_equals(s) {
                        if (loops.length > 0) {
                            return loops.itemAt(track.selected_loop).state_mgr.state === s
                        }
                        return false
                    }

                    paused: active_loop_state_equals(LoopState.LoopState.Paused)
                    muted: active_loop_state_equals(LoopState.LoopState.Muted)
                }

                Connections {
                    target: trackctlwidget
                    function onRecord() { loops.itemAt(track.selected_loop).manager.doRecord() }
                    function onPause() { loops.itemAt(track.selected_loop).manager.doPlayPause() }
                    function onUnpause() { loops.itemAt(track.selected_loop).manager.doPlayPause() }
                    function onMute() { loops.itemAt(track.selected_loop).manager.doMute() }
                    function doUnmute() { loops.itemAt(track.selected_loop).manager.doUnmute() }
                }
            }
        }
    }
}
