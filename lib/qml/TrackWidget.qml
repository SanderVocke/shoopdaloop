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
    property int selected_loop
    property int maybe_master_loop_idx: -1 //-1 is none
    property var master_loop_manager
    property var loop_managers
    property var loop_names
    property string name: ''
    property bool name_editable: true

    // Array of loop idxs
    property var loops_of_selected_scene: []
    property var loops_of_hovered_scene: []
    property var active_loop_state: selected_loop >= 0 ? loop_managers[selected_loop].state : LoopState.LoopState.Unknown

    width: childrenRect.width
    height: childrenRect.height

    signal set_loop_in_scene(int idx)
    signal renamed(string name)
    signal request_select_loop(int idx)
    signal request_load_wav(int idx, string wav_file)
    signal request_save_wav(int idx, string wav_file)
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

                        loop_idx: track.first_index + index
                        osc_link_obj: osc_link
                        is_selected: track.selected_loop === index
                        is_master: track.maybe_master_loop_idx === index
                        is_in_selected_scene: track.loops_of_selected_scene.includes(index)
                        is_in_hovered_scene: track.loops_of_hovered_scene.includes(index)
                        manager: track.loop_managers[index]
                        name: track.loop_names[index]

                        onSelected: () => { track.request_select_loop(index) }
                        onAdd_to_scene: () => { track.set_loop_in_scene(index) }
                        onRequest_load_wav: (wav_file) => { track.request_load_wav(index, wav_file) }
                        onRequest_save_wav: (wav_file) => { track.request_save_wav(index, wav_file) }
                        onRequest_rename: (name) => { track.request_rename_loop(index, name) }
                        onRequest_clear: () => { track.request_clear_loop(index) }
                    }
                }

                TrackControlWidget {
                    id: trackctlwidget

                    muted: track.active_loop_state === LoopState.LoopState.Muted
                }

                Connections {
                    target: trackctlwidget
                    function onRecord() {
                        if (track.active_loop_state === LoopState.LoopState.Recording ||
                            track.active_loop_state === LoopState.LoopState.Inserting ||
                            track.active_loop_state === LoopState.LoopState.RecordingWet) {
                            track.loop_managers[track.selected_loop].doStopRecord()
                        } else {
                            // Record on the selected loop and mute the others
                            track.actions_on_loop_mgrs(track.selected_loop,
                                                   (mgr) => { mgr.doRecord() },
                                                   (mgr) => { mgr.doMute() })
                        }
                    }
                    function onRecordFx() {
                        if (track.active_loop_state === LoopState.LoopState.Recording ||
                            track.active_loop_state === LoopState.LoopState.Inserting ||
                            track.active_loop_state === LoopState.LoopState.RecordingWet) {
                            track.loop_managers[track.selected_loop].doStopRecord()
                        } else {
                            // Record FX on the selected loop and mute the others
                            track.actions_on_loop_mgrs(track.selected_loop,
                                                   (mgr) => { mgr.doRecordFx(track.master_loop_manager) },
                                                   (mgr) => { mgr.doMute() })
                        }
                    }
                    function onRecordNCycles(n) {
                        if (track.active_loop_state === LoopState.LoopState.Recording ||
                            track.active_loop_state === LoopState.LoopState.Inserting ||
                            track.active_loop_state === LoopState.LoopState.RecordingWet) {
                            track.loop_managers[track.selected_loop].doStopRecord()
                        } else {
                            // Record on the selected loop and mute the others
                            track.actions_on_loop_mgrs(track.selected_loop,
                                                   (mgr) => { mgr.doRecordNCycles(n, track.master_loop_manager) },
                                                   (mgr) => { mgr.doMute() })
                        }
                    }
                    function onPlayLiveFx() {
                        // Play with live FX on the selected loop and mute the others
                        track.actions_on_loop_mgrs(track.selected_loop,
                                                (mgr) => { mgr.doPlayLiveFx() },
                                                (mgr) => { mgr.doMute() })
                    }
                    function onPause() {
                        track.loop_managers[track.selected_loop].doPause()
                    }
                    function onPlay() {
                        track.loop_managers[track.selected_loop].doPlay()
                    }
                    function onMute() {
                        for(var idx = 0; idx < track.num_loops; idx++) {
                            track.loop_managers[idx].doMute()
                        }
                    }
                    function onUnmute() {
                        track.loop_managers[track.selected_loop].doUnmute()
                    }
                }
            }
        }
    }
}
