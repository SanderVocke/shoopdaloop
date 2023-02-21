import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import '../../build/types.js' as Types

// The track widget displays the state of a track (collection of
// loopers with shared settings/control).
Item {
    id: track

    property int num_slots
    property LoopWidget master_loop
    property LoopWidget targeted_loop

    property string name: ''
    property bool name_editable: true

    property list<LoopWidget> loops_of_selected_scene: []
    property list<LoopWidget> loops_of_hovered_scene:  []

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

    // TODO: make ports dynamic
    AudioPortPair {
        id: dry_audio_l
        input_name_hint: 'audio_in_l'
        output_name_hint: 'audio_send_l'
    }
    AudioPortPair {
        id: dry_audio_r
        input_name_hint: 'audio_in_r'
        output_name_hint: 'audio_send_r'
    }
    MidiPortPair {
        id: dry_midi
        input_name_hint: 'midi_in'
        output_name_hint: 'midi_send'
    }
    AudioPortPair {
        id: wet_audio_l
        input_name_hint: 'audio_return_l'
        output_name_hint: 'audio_out_l'
    }
    AudioPortPair {
        id: wet_audio_r
        input_name_hint: 'audio_return_r'
        output_name_hint: 'audio_out_r'
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
                    model: track.num_slots
                    id: loops
                    width: childrenRect.width
                    height: childrenRect.height

                    LoopWidget {
                        id: lwidget
                        //name: track.loop_names[index]
                        //is_in_selected_scene: track.loops_of_selected_scene.includes(index)
                        //is_in_hovered_scene: track.loops_of_hovered_scene.includes(index)
                        name: 'Loop ' + (index+1).toString()

                        master_loop: null
                        targeted_loop: null

                        direct_port_pairs: []
                        dry_port_pairs: [ dry_audio_l, dry_audio_r, dry_midi ]
                        wet_port_pairs: [ wet_audio_l, wet_audio_r ]

                        //onToggle_in_current_scene: () => { track.toggle_loop_in_scene(index) }
                        //onRequest_rename: (name) => { track.request_rename_loop(index, name) }
                        //onRequest_clear: () => { track.request_clear_loop(index) }
                        //onRequest_toggle_selected: () => { track.request_toggle_loop_selected(index) }
                        //onRequest_set_as_targeted: () => { track.request_set_targeted_loop(index) }
                    }
                }

                Item {
                    height: 5
                    width: 10
                }

                TrackControlWidget {
                    id: trackctlwidget

                    //muted: track.active_loop_state === StatesAndActions.LoopMode.Muted
                    //ports_manager: track.ports_manager
                }
            }
        }
    }
}
