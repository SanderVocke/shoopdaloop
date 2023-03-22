import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import '../backend/frontend_interface/types.js' as Types

// The track widget displays the state of a track (collection of
// loopers with shared settings/control).
Item {
    id: track

    property var initial_descriptor : null
    property Registry objects_registry : null
    property Registry state_registry : null

    readonly property string obj_id : initial_descriptor.id

    property bool loaded : false
    property int n_loops_loaded : 0
    //audio_ports_repeater.loaded && midi_ports_repeater.loaded && loops.loaded

    signal rowAdded()
    signal requestDelete()

    SchemaCheck {
        descriptor: track.initial_descriptor
        schema: 'track.1'
    }

    function actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to) {
        var all_loops = Array.from(Array(loops.length).keys()).map((i) => loops[i])
        return {
            'schema': 'track.1',
            'id': obj_id,
            'name': name,
            'ports': all_ports().map((p) => p.actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to)),
            'loops': all_loops.map((l) => l.actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to))
        }
    }
    function queue_load_tasks(data_files_dir, add_tasks_to) {
        var all_loops = Array.from(Array(loops.length).keys()).map((i) => loops[i])
        all_loops.forEach((loop) => loop.queue_load_tasks(data_files_dir, add_tasks_to))
        all_ports().forEach((port) => port.queue_load_tasks(data_files_dir, add_tasks_to))
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

    function update_loop_port_connections() {
        for(var i=0; i<track.loops.length; i++) {
            var loop = track.loops[i]

        }
    }

    Component.onCompleted: {
        if(objects_registry) { objects_registry.register(obj_id, this) }
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
        // Descriptor is automatically determined from the previous loop...
        var prev_loop = track.loops[track.loops.length - 1]
        var prev_desc = prev_loop.initial_descriptor
        // ...id
        var prev_id = prev_desc.id
        var id_parts = prev_id.split("_")
        var prev_idx = parseInt(id_parts[id_parts.length - 1])
        var id_base = id_parts.slice(0, id_parts.length - 1).join("_")
        var id = id_base + "_" + (prev_idx+1).toString()
        // ...channels
        var channel_descriptors = []
        for(var i=0; i<prev_desc.channels.length; i++) {
            var prev_chan = prev_desc.channels[i]
            var chan = JSON.parse(JSON.stringify(prev_chan))
            // Note: assuming loop ID is always in the channel IDs
            chan.id = prev_chan.id.replace(prev_id, id)
            if (chan.id == prev_chan.id) { throw new Error("Did not find loop ID in channel ID") }
            channel_descriptors.push(chan)
        }
        var loop_descriptor = {
            'schema': 'loop.1',
            'id': id,
            'length': 0,
            'is_master': false,
            'channels': channel_descriptors
        }

        console.log("DESCRIPTOR", JSON.stringify(loop_descriptor))

        track.add_loop({
            initial_descriptor: loop_descriptor,
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
    function audio_ports() {
        return audio_ports_repeater.all_items();
    }

    function midi_ports() {
        return midi_ports_repeater.all_items();
    }

    function all_ports() {
        return [...audio_ports(), ...midi_ports()];
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

                Item {
                    anchors {
                        left: parent.left
                        right: parent.right
                    }
                    height: childrenRect.height

                    TextField {
                        anchors {
                            top: parent.top
                            left: parent.left
                            right: menubutton.left
                        }

                        text: track.name
                        font.pixelSize: 13
                        readOnly: !track.name_editable

                        onEditingFinished: () => {
                                            background_focus.forceActiveFocus()
                                            track.name = text
                                        }
                    }
        
                    Button {
                        id: menubutton
                        anchors {
                            top: parent.top
                            right: parent.right
                        }

                        width: 18
                        height: 28
                        MaterialDesignIcon {
                            size: 18
                            name: 'dots-vertical'
                            color: Material.foreground
                            anchors.centerIn: parent
                        }
                        onClicked: menu.open()

                        Menu {
                            id: menu

                            MenuItem {
                                text: "Delete"
                                onClicked: { track.requestDelete() }
                            }
                        }
                    }
                }

                Column {
                    spacing: 2
                    id: loops_column
                    height: childrenRect.height
                    width: childrenRect.width

                    // Note: loops injected here
                }

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
