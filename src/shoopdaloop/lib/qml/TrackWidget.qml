import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6
import QtQuick.Window
import ShoopDaLoop.PythonLogger

import ShoopConstants
import "../generate_session.js" as GenerateSession

// The track widget displays the state of a track (collection of
// loopers with shared settings/control).
Item {
    id: root

    width: width_adjuster.x + width_adjuster.width
    function setWidth(width) {
        width_adjuster.x = width - width_adjuster.width
    }
    
    anchors {
        top: parent ? parent.top : undefined
        bottom: parent ? parent.bottom : undefined
    }

    property var initial_descriptor : null
    readonly property PythonLogger logger : PythonLogger { name: "Frontend.Qml.TrackWidget" }

    property int track_idx: -1

    readonly property string obj_id : initial_descriptor.id

    property bool loaded : audio_ports_repeater.loaded && midi_ports_repeater.loaded && loops_loaded

    property var incubating_loops: []
    property bool loops_loaded : false
    property bool track_ready : loops_loaded
    Item {
        width: 0
        height: 0
        visible: false
        id: incubating_loops_parent
    }

    function checkIncubatingLoops() {
        if (incubating_loops.length == 0) {
            return
        }

        function loop_loaded(incubator) {
            if(incubator.status == Component.Loading) {
                return false;
            }
            if(incubator.object && incubator.object.loaded == false) {
                return false;
            }
            return true;
        }

        for(var i=0; i<incubating_loops.length; i++) {
            if (!loop_loaded(incubating_loops[i])) {
                return;
            }
        }

        let new_loops = incubating_loops.filter((l) => l.status == Component.Ready).map((l) => l.object)
        new_loops.forEach((l) => {
            l.parent = loops_column
        })
        incubating_loops = []
        root.loops_loaded = true
    }

    // The sync loop has its own special track widget,
    // which is mostly the same but more limited.
    property bool sync_loop_layout: false

    signal rowAdded()
    signal requestDelete()

    readonly property string object_schema : 'track.1'
    SchemaCheck {
        descriptor: root.initial_descriptor
        schema: root.object_schema
        object_description: `track ${track_idx}`
    }

    property var maybe_fx_chain: fx_chain_loader.active && fx_chain_loader.status == Loader.Ready ? fx_chain_loader.item : undefined

    function actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to) {
        var all_loops = Array.from(Array(loops.length).keys()).map((i) => loops[i])
        var rval = {
            'schema': 'track.1',
            'id': obj_id,
            'name': name,
            'width': Math.round(width),
            'ports': ports.map((p) => p.actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to)),
            'loops': all_loops.map((l) => l.actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to)),
        }
        if (maybe_fx_chain) { rval['fx_chain'] = maybe_fx_chain.actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to) }
        return rval
    }
    function queue_load_tasks(data_files_dir, from_sample_rate, to_sample_rate, add_tasks_to) {
        var all_loops = Array.from(Array(loops.length).keys()).map((i) => loops[i])
        all_loops.forEach((loop) => loop.queue_load_tasks(data_files_dir, from_sample_rate, to_sample_rate, add_tasks_to))
        ports.forEach((port) => port.queue_load_tasks(data_files_dir, from_sample_rate, to_sample_rate, add_tasks_to))
    }

    function qml_close() {
        root.logger.debug(`QML close ${root.name}`)
        reg_entry.close()
        ports.forEach(p => p.qml_close())
        for(var i=0; i<loops.length; i++) {
            loops[i].qml_close();
        }
    }
    
    readonly property int num_slots : loops.length
    property string name: initial_descriptor.name
    property int max_slots
    property bool name_editable: true
    readonly property string port_name_prefix: ''
    readonly property var audio_port_descriptors : initial_descriptor.ports.filter(p => p.schema == 'audioport.1')
    readonly property var midi_port_descriptors : initial_descriptor.ports.filter(p => p.schema == 'midiport.1')
    readonly property var fx_chain_descriptor : 'fx_chain' in initial_descriptor ? initial_descriptor.fx_chain : undefined
    readonly property var loop_descriptors : initial_descriptor ? initial_descriptor.loops : []

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
            var loop_incubator = loop_factory.incubateObject(incubating_loops_parent, properties);
            incubating_loops.push(loop_incubator)
            function on_status(status) {
                if (status == Component.Ready) {
                    let loop = loop_incubator.object
                    loop.track_obj_id = Qt.binding(() => root.obj_id)
                    checkIncubatingLoops()
                }
            }
            loop_incubator.onStatusChanged = on_status
            checkIncubatingLoops()
        }
    }

    function update_loop_port_connections() {
        for(var i=0; i<root.loops.length; i++) {
            var loop = root.loops[i]

        }
    }

    // Draggy rect for moving the track
    Rectangle {
        id: track_mover
        anchors {
            left: parent.left
            right: width_adjuster.left
            top: parent.top
            bottom: parent.bottom
        }

        // for debugging
        // color: 'yellow'
        color: 'transparent'

        Item {
            id: movable
            width: root.width
            height: root.height
            parent: Overlay.overlay
            visible: move_area.drag.active
            z: 3

            Drag.active: move_area.drag.active
            Drag.hotSpot.x : width/2
            Drag.hotSpot.y : height/2
            Drag.source: root
            Drag.keys: ['TrackWidget']

            Rectangle {
                anchors {
                    top: parent.top
                    horizontalCenter: parent.horizontalCenter
                }
                width: 3
                height: parent.height
                color: 'red'
                opacity: 0.3
            }
            Rectangle {
                anchors.fill: parent
                color: 'white'
                opacity: 0.5

                Image {
                    id: movable_image
                    source: ''
                }
            }

            function resetCoords() {
                x = track_mover.mapToItem(Overlay.overlay, 0, 0).x
                y = track_mover.mapToItem(Overlay.overlay, 0, 0).y
            }
            Component.onCompleted: resetCoords()
            onVisibleChanged: resetCoords()
        }

        MouseArea {
            id: move_area
            anchors.fill: parent
            cursorShape: Qt.SizeAllCursor

            onReleased: movable.Drag.drop()
            onPressed: movable.resetCoords()

            drag {
                target: movable
                onActiveChanged: {
                    if (active) {
                        root.grabToImage((result) => {
                            movable_image.source = result.url
                        })
                    }
                }
            }
        }
    }

    // Draggy rect for width ajustment
    Rectangle {
        id: width_adjuster
        height: root.height
        width: 10
        x: 100

        // for debugging
        // color: 'red'
        color: 'transparent'

        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.SizeHorCursor

            drag {
                axis: "XAxis"
                target: width_adjuster
                minimumX: 20
                maximumX: 400
            }
        }
    }

    RegisterInRegistry {
        id: reg_entry
        registry: registries.objects_registry
        object: root
        key: root.obj_id
    }

    RegistryLookup {
        id: lookup_control_widget
        registry: registries.objects_registry
        key: root.obj_id + "_control_widget"
    }
    property alias control_widget: lookup_control_widget.object

    Component.onCompleted: {
        if (initial_descriptor && initial_descriptor.width != undefined) {
            setWidth(initial_descriptor.width)
        }
        // Instantiate initial loops
        root.loop_descriptors.forEach((desc, idx) => {
            root.add_loop({
                initial_descriptor: desc,
                track_idx: Qt.binding( () => root.track_idx ),
                all_loops_in_track: Qt.binding( () => root.loops ),
                maybe_fx_chain: Qt.binding( () => root.maybe_fx_chain )
            });
        })
    }

    function add_default_loop() {
        // Descriptor is automatically determined from the previous loop...
        var prev_desc
        for(var i=root.loops.length; i>=0; i--) {
            let l = root.loops[i]
            let desc = l && l.initial_descriptor ? l.initial_descriptor : undefined
            if (desc) { prev_desc = desc; break; }
        }
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
            initial_descriptor: loop_descriptor
        });
    }

    function add_row() {
        add_default_loop();
        rowAdded()
    }

    MapperWithLoadedDetection {
        id : audio_ports_repeater
        model : root.audio_port_descriptors

        onAboutToAdd: (descriptor, index) => root.logger.debug(`Track ${root}: Creating audio port ${descriptor.id} with name hint ${descriptor.name_parts.join('')}`)

        AudioPort {
            property var mapped_item
            property int index
            descriptor: mapped_item
            is_internal: false
        }
    }
    MapperWithLoadedDetection {
        id : midi_ports_repeater
        model : root.midi_port_descriptors

        onAboutToAdd: (descriptor, index) => root.logger.debug(`Track ${root}: Creating MIDI port ${descriptor.id} with name hint ${descriptor.name_parts.join('')}`)

        MidiPort {
            property var mapped_item
            property int index
            descriptor: mapped_item
            is_internal: false
        }
    }

    // Use registry lookup to find our ports back dynamically
    RegistryLookups {
        id: lookup_ports
        registry: registries.objects_registry
        keys: root.initial_descriptor ? root.initial_descriptor.ports.map((p) => p.id) : []
    }
    property alias ports : lookup_ports.objects
    function is_audio(p) { return p.schema.match(/audioport\.[0-9]+/) }
    function is_midi(p)  { return p.schema.match(/midiport\.[0-9]+/)  }
    function is_in(p)    { return p.id.match(/.*_(?:in|direct)(?:_[0-9]*)?$/); }
    function is_out(p)   { return p.id.match(/.*_(?:out|direct)(?:_[0-9]*)?$/); }
    function is_send(p)  { return p.id.match(/.*_(?:send)(?:_[0-9]*)?$/); }
    function is_return(p)  { return p.id.match(/.*_(?:return)(?:_[0-9]*)?$/); }
    readonly property var audio_ports : ports.filter(p => p && is_audio(p.descriptor))
    readonly property var midi_ports : ports.filter(p => p && is_midi(p.descriptor))
    readonly property var input_ports : ports.filter(p => p && is_in(p.descriptor))
    readonly property var output_ports : ports.filter(p => p && is_out(p.descriptor))
    readonly property var audio_in_ports : audio_ports.filter(p => p && is_in(p.descriptor))
    readonly property var audio_out_ports : audio_ports.filter(p => p && is_out(p.descriptor))
    readonly property var audio_send_ports : audio_ports.filter(p => p && is_send(p.descriptor))
    readonly property var audio_return_ports : audio_ports.filter(p => p && is_return(p.descriptor))
    readonly property var midi_in_ports : midi_ports.filter(p => p && is_in(p.descriptor))
    readonly property var midi_out_ports : midi_ports.filter(p => p && is_out(p.descriptor))
    readonly property var midi_send_ports : midi_ports.filter(p => p && is_send(p.descriptor))

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

            Component.onCompleted: {
                root.fx_ready = Qt.binding(() => this.ready)
                root.fx_active = Qt.binding(() => this.active)
            }
        }
    }

    Item {
        anchors.fill: parent

        BusyIndicator {
            anchors.centerIn: parent
            running: true
            visible: !root.track_ready
        }
    }

    Item {
        visible: root.track_ready
        property int x_spacing: 8
        property int y_spacing: 4

        anchors {
            left: parent.left
            right: parent.right
            top: parent.top
        }
        height: childrenRect.height + y_spacing

        Item {
            anchors {
                left: parent.left
                right: parent.right
                top: parent.top
            }

            height: childrenRect.height

            Column {
                id: track_column
                spacing: 2
                anchors {
                    left: parent.left
                    right: parent.right
                }

                Item {
                    anchors {
                        left: parent.left
                        right: parent.right
                    }
                    height: childrenRect.height

                    ShoopTextField {
                        id: title_field
                        visible: root.name_editable

                        anchors {
                            top: parent.top
                            left: parent.left
                            right: menubutton.left
                            rightMargin: 3
                            leftMargin: 2
                            topMargin: 2
                        }

                        height: 26

                        text: root.name
                        onEditingFinished: () => {
                                            focus = false
                                            release_focus_notifier.notify()
                                            root.name = text
                                        }
                    }
                    Item {
                        visible: !root.name_editable
                        anchors.fill: title_field

                        Label {
                            text: root.name
                            font.pixelSize: 12
                            anchors.centerIn: parent
                        }
                    }
        
                    ExtendedButton {
                        tooltip: "Track options."
                        id: menubutton
                        anchors {
                            verticalCenter : title_field.verticalCenter
                            right: parent.right
                            rightMargin: 2
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

                            ShoopMenuItem {
                                text: "Connections..."
                                onClicked: { connectionsdialog.open() }
                            }

                            ShoopMenuItem {
                                text: "Delete Track"
                                shown: !root.sync_loop_layout
                                onClicked: { root.requestDelete() }
                            }

                            ShoopMenuItem {
                                text: "Snapshot FX State"
                                shown: root.maybe_fx_chain != undefined
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
                                    width: 300
                                    onAcceptedInput: name => {
                                        var id = registries.fx_chain_states_registry.generate_id("fx_chain_state")
                                        data.title = name
                                        registries.fx_chain_states_registry.register(id, JSON.parse(JSON.stringify(data)))
                                    }
                                }
                            }

                            Menu {
                                id: restore_submenu
                                title: "Restore FX State"
                                enabled: !root.sync_loop_layout && root.maybe_fx_chain != undefined && fx_states.length > 0

                                RegistrySelects {
                                    registry: registries.fx_chain_states_registry
                                    select_fn: r => true
                                    values_only: true
                                    id: all_chain_states
                                }

                                property var fx_states: all_chain_states.objects
                                    .filter(v => v.title != "")

                                Repeater {
                                    model: restore_submenu.fx_states.length

                                    ShoopMenuItem {
                                        property var mapped_item: restore_submenu.fx_states[index]
                                        text: mapped_item.title
                                        shown: root.fx_chain_descriptor != undefined && (mapped_item.type == root.fx_chain_descriptor.type)
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

                Item {
                    // To subtract open space in the sync
                    // loop layout
                    width: 1
                    height: root.sync_loop_layout ? -24 : 0
                }

                Item {
                    height: loops_column.height
                    anchors {
                        left: parent.left
                        right: parent.right
                    }

                    Column {
                        spacing: 2
                        id: loops_column
                        height: childrenRect.height
                        
                        anchors {
                            left: parent.left
                            right: parent.right
                        }

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

                    // Repeater for the drag'n'drop drop areas to drop loops in.
                    // These drop areas are in-between the loops and can be used to reorder them.
                    Repeater {
                        id: drop_areas_repeater
                        anchors.fill: parent
                        model: root.loops.length + 1

                        DropArea {
                            id: drop_area
                            width: root.width
                            height: 20
                            keys: ['LoopWidget_track_' + root.obj_id]

                            property var above_loop : index > 0 ? root.loops[index-1] : null
                            property var below_loop : index < root.loops.length ? root.loops[index] : null

                            onDropped: (event) => {
                                let src_loop = drag.source
                                let src_loop_idx = src_loop.idx_in_track
                                let loops = root.loops
                                var new_loops = []
                                for (var i=0; i<loops.length+1; i++) {
                                    if (i == index) {
                                        new_loops.push(src_loop)
                                    }
                                    if (i != src_loop_idx && i < loops.length) {
                                        new_loops.push(loops[i])
                                    }
                                }
                                root.loops = new_loops
                            }

                            y : {
                                if (above_loop) {
                                    above_loop.y // dummy dependency
                                    return above_loop.mapToItem(loops_column, 0, 0).y + above_loop.height + loops_column.spacing/2 - height/2
                                }
                                if (below_loop) {
                                    below_loop.y // dummy dependency
                                    return below_loop.mapToItem(loops_column, 0, 0).y - loops_column.spacing/2 - height/2
                                }
                                return 0
                            }
                            x : 0

                            Rectangle {
                                color: 'white'
                                opacity: 0.7
                                visible : (parent.above_loop || parent.below_loop) && drop_area.containsDrag
                                anchors.fill: parent
                            }
                        }
                    }
                }

                ExtendedButton {
                    tooltip: "Add a loop to track(s)."
                    visible: !root.sync_loop_layout
                    
                    anchors {
                        left: parent.left
                        right: parent.right
                        leftMargin: 6
                        rightMargin: 6
                    }

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
        id: connectionsdialog
        title: root.name + " Connections"

        audio_in_ports : root.audio_in_ports
        audio_out_ports : root.audio_out_ports
        audio_send_ports: root.audio_send_ports
        audio_return_ports: root.audio_return_ports
        midi_in_ports : root.midi_in_ports
        midi_out_ports : root.midi_out_ports
        midi_send_ports: root.midi_send_ports
    }

}
