import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import '../../build/types.js' as Types

// The track widget displays the state of a track (collection of
// loopers with shared settings/control).
Item {
    id: track

    property var initial_descriptor : null
    property Registry objects_registry : null
    property Registry state_registry : null

    property bool loaded : false
    property int n_loops_loaded : 0
    //audio_ports_repeater.loaded && midi_ports_repeater.loaded && loops.loaded

    signal rowAdded()

    onLoadedChanged: if(loaded) { console.log("LOADED: TrackWidget")}

    SchemaCheck {
        descriptor: track.initial_descriptor
        schema: 'track.1'
    }

    function qml_close() {
        objects_registry.unregister(initial_descriptor.id)
        all_ports().forEach(p => p.qml_close())
        for(var i=0; i<loops.length; i++) {
            loops[i].qml_close();
        }
    }
    
    readonly property int num_slots : loops.length
    property string name: initial_descriptor.name
    property int max_slots
    readonly property bool name_editable: true
    readonly property string port_name_prefix: ''
    readonly property var audio_port_descriptors : initial_descriptor.ports.filter(p => p.schema == 'audioport.1')
    readonly property var midi_port_descriptors : initial_descriptor.ports.filter(p => p.schema == 'midiport.1')
    readonly property var loop_descriptors : initial_descriptor.loops

    readonly property var loop_factory : Qt.createComponent("LoopWidget.qml")
    property alias loops : loops_column.children

    function add_loop(properties) {
        if (loop_factory.status == Component.Error) {
            throw new Error("TrackWidget: Failed to load loop factory: " + loop_factory.errorString())
        } else if (loop_factory.status != Component.Ready) {
            throw new Error("TrackWidget: Loop factory not ready")
        } else {
            var loop = loop_factory.createObject(loops_column, properties);
            loop.onLoadedChanged.connect(() => loop_loaded_changed(loop))
            return loop
        }
    }

    Component.onCompleted: {
        if(objects_registry) { objects_registry.register(initial_descriptor.id, this) }
        loaded = false
        var _n_loops_loaded = 0
        // Instantiate initial loops
        track.loop_descriptors.forEach(desc => {
            var loop = track.add_loop({
                initial_descriptor: desc,
                objects_registry: track.objects_registry,
                state_registry: track.state_registry
            });
            if (loop.loaded) { _n_loops_loaded += 1 }
        })
        n_loops_loaded = _n_loops_loaded
        loaded = Qt.binding(() => { return n_loops_loaded >= num_slots })
    }

    function loop_loaded_changed(loop) {
        if(loop.loaded) {
            n_loops_loaded = n_loops_loaded + 1;
        } else {
            n_loops_loaded = n_loops_loaded - 1;
        }
    }

    width: childrenRect.width
    height: childrenRect.height

    function add_row() {
        var id = track.initial_descriptor + '_loop_0';
        if (track.loops.length > 0) {
            // Automatically determine loop id based on the previous ones
            var prev_loop = track.loops[track.loops.length - 1]
            var id_parts = prev_loop.initial_descriptor.id.split("_")
            var prev_id = parseInt(id_parts[id_parts.length - 1])
            var id_base = id_parts.slice(0, id_parts.length - 1).join("_")
            id = id_base + "_" + (prev_id+1).toString()
        }

        track.add_loop({
            initial_descriptor: ({
                'schema': 'loop.1',
                'id': id,
                'length': 0,
                'is_master': false,
                'channels': [] //FIXME
            }),
            objects_registry: track.objects_registry,
            state_registry: track.state_registry
        });

        rowAdded()
    }

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

    RepeaterWithLoadedDetection {
        id : audio_ports_repeater
        model : track.audio_port_descriptors.length

        AudioPort {
            descriptor: track.audio_port_descriptors[index]
            objects_registry: track.objects_registry
            state_registry: track.state_registry
        }
    }
    RepeaterWithLoadedDetection {
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
            ...audio_ports_repeater.all_items(),
            ...midi_ports_repeater.all_items()
        ]
    }

    Item {
        property int x_spacing: 8
        property int y_spacing: 4

        width: childrenRect.width + x_spacing
        height: childrenRect.height + y_spacing

        Item {
            width: childrenRect.width
            height: childrenRect.height
            x: parent.x_spacing/2
            y: parent.y_spacing/2

            Column {
                id: track_column
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

                Column {
                    spacing: 2
                    id: loops_column
                    height: childrenRect.height
                    width: childrenRect.width

                    // Note: loops injected here
                }

                // RepeaterWithLoadedDetection {
                //     model: track.num_slots
                //     id: loops
                //     width: childrenRect.width
                //     height: childrenRect.height

                //     LoopWidget {
                //         initial_descriptor : track.loop_descriptors[index]
                //         objects_registry : track.objects_registry
                //         state_registry : track.state_registry
                //     }
                // }

                Button {
                    width: 100
                    height: 30
                    MaterialDesignIcon {
                        size: 20
                        name: 'plus'
                        color: Material.foreground
                        anchors.centerIn: parent
                    }

                    onClicked: track.add_row()
                }
            }
        }
    }
}
