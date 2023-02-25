import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import '../../build/types.js' as Types

// The track widget displays the state of a track (collection of
// loopers with shared settings/control).
Item {
    id: track

    property var descriptor : null
    property Registry objects_registry : null
    property Registry state_registry : null

    SchemaCheck {
        descriptor: track.descriptor
        schema: 'track.1'
    }
    Component.onCompleted: if(objects_registry) { objects_registry.register(descriptor.id, this) }
    function qml_close() {
        objects_registry.unregister(descriptor.id)
        all_ports().forEach(p => p.qml_close())
        for(var i=0; i<loops.model; i++) {
            loops.itemAt(i).qml_close();
        }
    }
    
    readonly property int num_slots : descriptor.loops.length
    property string name: descriptor.name
    readonly property bool name_editable: true
    readonly property string port_name_prefix: ''
    readonly property var audio_port_descriptors : descriptor.ports.filter(p => p.schema == 'audioport.1')
    readonly property var midi_port_descriptors : descriptor.ports.filter(p => p.schema == 'midiport.1')
    readonly property var loop_descriptors : descriptor.loops

    width: childrenRect.width
    height: childrenRect.height

    // signal toggle_loop_in_scene(var loop)
    // signal renamed(string name)
    // signal request_select_loop(var loop)
    // signal request_rename_loop(var loop, string name)
    // signal request_clear_loop(var loop)
    // signal request_toggle_loop_selected(var loop)
    // signal request_set_targeted_loop(var loop)
    // signal loop_created(int index, LoopWidget loop)

    // function actions_on_loop_mgrs(idx, on_idx_loop_fn, on_other_loop_fn) {
    //     for(var i = 0; i < track.num_loops; i++) {
    //         var mgr = loop_managers[i]
    //         if (idx === i) {
    //             on_idx_loop_fn(mgr)
    //         }
    //         else {
    //             on_other_loop_fn(mgr)
    //         }
    //     }
    // }

    // TODO: make ports dynamic
    // TODO: apparently the order in which these are instantiated will make
    // Patchance group the pairs or not. Quite confusing...

    Repeater {
        id : audio_ports_repeater
        model : track.audio_port_descriptors.length

        AudioPort {
            descriptor: track.audio_port_descriptors[index]
            objects_registry: track.objects_registry
            state_registry: track.state_registry
        }
    }
    Repeater {
        id : midi_ports_repeater
        model : track.midi_port_descriptors.length

        MidiPort {
            descriptor: track.midi_port_descriptors[index]
            objects_registry: track.objects_registry
            state_registry: track.state_registry
        }
    }
    function all_ports() {
        return [
            ...Array.from(Array(audio_ports_repeater.model).keys()).map(i => audio_ports_repeater.itemAt(i)),
            ...Array.from(Array(midi_ports_repeater.model).keys()).map(i => midi_ports_repeater.itemAt(i))
        ]
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
                                           track.name = text
                                       }
                }

                Repeater {
                    model: track.num_slots
                    id: loops
                    width: childrenRect.width
                    height: childrenRect.height

                    LoopWidget {
                        descriptor : track.loop_descriptors[index]
                        objects_registry : track.objects_registry
                        state_registry : track.state_registry

                        // id: lwidget
                        // //name: track.loop_names[index]
                        // //is_in_selected_scene: track.loops_of_selected_scene.includes(index)
                        // //is_in_hovered_scene: track.loops_of_hovered_scene.includes(index)
                        // name: 'Loop ' + (index+1).toString()
                        // master_loop: track.master_loop
                        // targeted_loop: track.targeted_loop

                        // direct_port_pairs: []
                        // dry_port_pairs: [ dry_audio_l, dry_audio_r, dry_midi ]
                        // wet_port_pairs: [ wet_audio_l, wet_audio_r ]

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
