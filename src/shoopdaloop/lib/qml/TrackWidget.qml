import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3

import '../generated/types.js' as Types
import "../generate_session.js" as GenerateSession

// The track widget displays the state of a track (collection of
// loopers with shared settings/control).
Item {
    id: root

    property var initial_descriptor : null
    property Registry objects_registry : null
    property Registry state_registry : null

    property int track_idx: -1

    RegistryLookup {
        id: fx_chain_states_registry_lookup
        registry: root.state_registry
        key: 'fx_chain_states_registry'
    }
    property alias fx_chain_states_registry : fx_chain_states_registry_lookup.object

    readonly property string obj_id : initial_descriptor.id

    property bool loaded : false
    property int n_loops_loaded : 0
    //audio_ports_repeater.loaded && midi_ports_repeater.loaded && loops.loaded

    signal rowAdded()
    signal requestDelete()

    readonly property string object_schema : 'track.1'
    SchemaCheck {
        descriptor: root.initial_descriptor
        schema: root.object_schema
    }

    property var maybe_fx_chain: fx_chain_loader.active && fx_chain_loader.status == Loader.Ready ? fx_chain_loader.item : undefined

    function actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to) {
        var all_loops = Array.from(Array(loops.length).keys()).map((i) => loops[i])
        var rval = {
            'schema': 'track.1',
            'id': obj_id,
            'name': name,
            'ports': ports.map((p) => p.actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to)),
            'loops': all_loops.map((l) => l.actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to)),
        }
        if (maybe_fx_chain) { rval['fx_chain'] = maybe_fx_chain.actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to) }
        return rval
    }
    function queue_load_tasks(data_files_dir, add_tasks_to) {
        var all_loops = Array.from(Array(loops.length).keys()).map((i) => loops[i])
        all_loops.forEach((loop) => loop.queue_load_tasks(data_files_dir, add_tasks_to))
        ports.forEach((port) => port.queue_load_tasks(data_files_dir, add_tasks_to))
    }

    function qml_close() {
        reg_entry.close()
        ports.forEach(p => p.qml_close())
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
    readonly property var fx_chain_descriptor : 'fx_chain' in initial_descriptor ? initial_descriptor.fx_chain : undefined
    readonly property var loop_descriptors : initial_descriptor.loops

    readonly property var loop_factory : Qt.createComponent("LoopWidget.qml")
    property alias loops : loops_column.children

    property bool fx_ready : false // Will be re-bound when FX instantiated
    property bool fx_active : false // Will be re-bound when FX instantiated

    function add_loop(properties) {
        if (loop_factory.status == Component.Error) {
            throw new Error("TrackWidget: Failed to load loop factory: " + loop_factory.errorString())
        } else if (loop_factory.status != Component.Ready) {
            throw new Error("TrackWidget: Loop factory not ready")
        } else {
            var loop = loop_factory.createObject(loops_column, properties);
            loop.onLoadedChanged.connect(() => {
                loop_loaded_changed(loop)
            })
            return loop
        }
    }

    function update_loop_port_connections() {
        for(var i=0; i<root.loops.length; i++) {
            var loop = root.loops[i]

        }
    }

    RegisterInRegistry {
        id: reg_entry
        registry: root.objects_registry
        object: root
        key: root.obj_id
    }

    Component.onCompleted: {
        loaded = false
        var _n_loops_loaded = 0
        // Instantiate initial loops
        root.loop_descriptors.forEach((desc, idx) => {
            var loop = root.add_loop({
                initial_descriptor: desc,
                objects_registry: root.objects_registry,
                state_registry: root.state_registry,
                track_widget: root,
                track_idx: Qt.binding( () => { return root.track_idx } )
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
        var prev_loop = root.loops[root.loops.length - 1]
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
        var name = "(" + (root.loops.length).toString() + ")"
        var loop_descriptor = GenerateSession.generate_loop(id, name, 0, false, channel_descriptors)

        root.add_loop({
            initial_descriptor: loop_descriptor,
            objects_registry: root.objects_registry,
            state_registry: root.state_registry
        });

        rowAdded()
    }

    RepeaterWithLoadedDetection {
        id : audio_ports_repeater
        model : root.audio_port_descriptors.length

        AudioPort {
            descriptor: root.audio_port_descriptors[index]
            state_registry: root.state_registry
            objects_registry: root.objects_registry
            is_internal: false
        }
    }
    RepeaterWithLoadedDetection {
        id : midi_ports_repeater
        model : root.midi_port_descriptors.length

        MidiPort {
            descriptor: root.midi_port_descriptors[index]
            state_registry: root.state_registry
            objects_registry: root.objects_registry
            is_internal: false
        }
    }

    // Use registry lookup to find our ports back dynamically
    RegistryLookups {
        id: lookup_ports
        registry: root.objects_registry
        keys: root.initial_descriptor ? root.initial_descriptor.ports.map((p) => p.id) : []
    }
    property alias ports : lookup_ports.objects
    function is_audio(p) { return p.schema.match(/audioport\.[0-9]+/) }
    function is_midi(p)  { return p.schema.match(/midiport\.[0-9]+/)  }
    function is_in(p)    { return p.direction == "input" && p.id.match(/.*_(?:in|direct)(?:_[0-9]*)?$/); }
    function is_out(p)   { return p.direction == "output" && p.id.match(/.*_(?:out|direct)(?:_[0-9]*)?$/); }
    readonly property var audio_ports : ports.filter(p => p && is_audio(p.descriptor))
    readonly property var midi_ports : ports.filter(p => p && is_midi(p.descriptor))
    readonly property var input_ports : ports.filter(p => p && is_in(p.descriptor))

    Loader {
        id: fx_chain_loader
        active: root.fx_chain_descriptor != undefined
        sourceComponent: fx_chain_component
    }
    Component {
        id: fx_chain_component
        FXChain {
            id: chain
            descriptor: root.fx_chain_descriptor
            state_registry: root.state_registry
            objects_registry: root.objects_registry

            Component.onCompleted: {
                root.fx_ready = Qt.binding(() => this.ready)
                root.fx_active = Qt.binding(() => this.active)
            }
        }
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

                    ShoopTextField {
                        anchors {
                            top: parent.top
                            left: parent.left
                            right: menubutton.left
                            rightMargin: 3
                        }

                        text: root.name
                        font.pixelSize: 13
                        readOnly: !root.name_editable

                        onEditingFinished: () => {
                                            background_focus.forceActiveFocus()
                                            root.name = text
                                        }
                    }
        
                    ExtendedButton {
                        tooltip: "Track options."
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
                                text: "Connections..."
                                onClicked: { root.openConnectionsDialog() }
                            }

                            MenuItem {
                                text: "Delete Track"
                                onClicked: { root.requestDelete() }
                            }

                            MenuItem {
                                text: "Snapshot FX State"
                                enabled: root.maybe_fx_chain != undefined
                                onClicked: {
                                    var snapshot = root.maybe_fx_chain.actual_session_descriptor()
                                    delete snapshot.ports
                                    snapshot_fx_state_dialog.data = snapshot
                                    snapshot_fx_state_dialog.open()
                                }

                                InputDialog {
                                    id: snapshot_fx_state_dialog
                                    property var data
                                    title: "Choose a name"
                                    onAcceptedInput: name => {
                                        var id = root.fx_chain_states_registry.generate_id("fx_chain_state")
                                        data.title = name
                                        root.fx_chain_states_registry.register(id, JSON.parse(JSON.stringify(data)))
                                    }
                                }
                            }

                            Menu {
                                id: restore_submenu
                                title: "Restore FX State"
                                enabled: root.maybe_fx_chain != undefined && fx_states.length > 0

                                RegistrySelects {
                                    registry: root.fx_chain_states_registry
                                    select_fn: r => true
                                    values_only: true
                                    id: all_chain_states
                                }

                                property list<var> fx_states: all_chain_states.objects
                                    .filter(v => v.title != "")

                                Repeater {
                                    model: restore_submenu.fx_states.length

                                    MenuItem {
                                        property var mapped_item: restore_submenu.fx_states[index]
                                        text: mapped_item.title
                                        enabled: root.fx_chain_descriptor && (mapped_item.type == root.fx_chain_descriptor.type)
                                        onClicked: root.maybe_fx_chain.restore_state(mapped_item.internal_state)
                                    }
                                }
                            }
                        }
                    }

                    ExtendedButton {
                        tooltip: "Open FX chain GUI if ready. Red = not ready. Grey = bypassed."
                        id: fxuibutton
                        visible: root.maybe_fx_chain != undefined

                        anchors {
                            top: menubutton.bottom
                            topMargin: -6
                            right: parent.right
                        }

                        width: 18
                        height: 28
                        Label {
                            text: "FX"
                            font.pixelSize: 10
                            color: (!root.fx_ready) ? "red" :
                                   root.fx_active ? Material.foreground :
                                   "grey"
                            anchors {
                                verticalCenter: parent.verticalCenter
                                horizontalCenter: parent.horizontalCenter
                            }
                        }

                        onClicked: { if (root.maybe_fx_chain != undefined) { root.maybe_fx_chain.set_ui_visible(!root.maybe_fx_chain.ui_visible) } }
                    }
                    
                }

                Column {
                    spacing: 2
                    id: loops_column
                    height: childrenRect.height
                    width: childrenRect.width

                    // Note: loops injected here
                    onChildrenChanged: update_coords()
                    function update_coords() {
                        // Update loop indexes
                        var idx=0
                        for(var i=0; i<children.length; i++) {
                            let c = children[i]
                            if (c instanceof LoopWidget) {
                                c.idx_in_track = idx;
                                idx++;
                            }
                        }
                    }
                }

                ExtendedButton {
                    tooltip: "Add a loop to track(s)."
                    width: 100
                    height: 30
                    MaterialDesignIcon {
                        size: 20
                        name: 'plus'
                        color: Material.foreground
                        anchors.centerIn: parent
                    }

                    onClicked: root.add_row()
                }
            }
        }
    }

    ConnectionsDialog {
        id: connections_dialog
        title: root.name + " Connections"
    }

}
