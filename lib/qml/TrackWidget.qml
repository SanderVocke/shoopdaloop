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
    property string name: ''

    // Array of loop idxs
    property var loops_of_selected_scene: []
    property var loops_of_hovered_scene: []

    width: childrenRect.width
    height: childrenRect.height

    signal set_loop_in_scene(int idx)
    signal renamed(string name)

    function get_loop_state(loop_idx) {
        if (loops && loops.model > 0 && loops.itemAt(loop_idx) !== null) {
            return loops.itemAt(loop_idx).manager.state
        } else {
            return LoopState.LoopState.Unknown
        }
    }
    function update_active_loop_state() {
        active_loop_state = get_loop_state(selected_loop)
        active_loop_stateChanged()
    }
    onSelected_loopChanged: { update_active_loop_state() }
    property var active_loop_state: get_loop_state(selected_loop)

    function actions_on_loops(idx, on_idx_loop_fn, on_other_loop_fn) {
        for(var i = 0; i < track.num_loops; i++) {
            var lp = loops.itemAt(i)
            if (idx === i) {
                on_idx_loop_fn(lp)
            }
            else {
                on_other_loop_fn(lp)
            }
        }
    }

    function do_select_loop(index) {
        // If we are playing another loop, tell SL to switch
        if(index >= 0 &&
           track.selected_loop !== index &&
           loops.itemAt(index).manager.state === LoopState.LoopState.Muted) {
            actions_on_loops(index, (lp) => { lp.manager.doUnmute() }, (lp) => { lp.manager.doMute() })
        }

        // Update everything else
        selected_loop = index
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
                        loop_idx: track.first_index + index
                        osc_link_obj: osc_link
                        is_selected: track.selected_loop === index
                        is_master: track.maybe_master_loop === index
                        is_in_selected_scene: track.loops_of_selected_scene.includes(index)
                        is_in_hovered_scene: track.loops_of_hovered_scene.includes(index)

                        onSelected: () => { track.do_select_loop(index) }
                        onAdd_to_scene: () => { track.set_loop_in_scene(index) }
                        onState_changed: () => { track.update_active_loop_state() }
                    }
                }

                TrackControlWidget {
                    id: trackctlwidget

                    paused: track.active_loop_state === LoopState.LoopState.Paused ||
                            track.active_loop_state === LoopState.LoopState.Unknown ||
                            track.active_loop_state === LoopState.LoopState.Off
                    muted: track.active_loop_state === LoopState.LoopState.Muted
                }

                Connections {
                    target: trackctlwidget
                    function onRecord() {
//                        var lp = loops.itemAt(track.selected_loop)
//                        lp.manager.doRecord()
                        track.actions_on_loops(track.selected_loop, (lp) => { lp.manager.doRecord() }, (lp) => { lp.manager.doMute() })
                    }
                    function onPause() {
                        var lp = loops.itemAt(track.selected_loop)
                        lp.manager.doPlayPause()
                    }
                    function onUnpause() {
                        var lp = loops.itemAt(track.selected_loop)
                        lp.manager.doTrigger()
                    }
                    function onMute() {
                        for(var idx = 0; idx < track.num_loops; idx++) {
                            loops.itemAt(idx).manager.doMute()
                        }
                    }
                    function doUnmute() {
                        var lp = loops.itemAt(track.selected_loop)
                        lp.manager.doUnmute()
                    }
                }
            }
        }
    }
}
