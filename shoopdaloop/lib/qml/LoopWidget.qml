import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15
import QtQuick.Dialogs

import '../backend/frontend_interface/types.js' as Types
import '../mode_helpers.js' as ModeHelpers
import '../stereo.js' as Stereo

// The loop widget allows manipulating a single loop within a track.
Item {
    id: root
    property var track_widget

    property var initial_descriptor : null
    property Registry objects_registry : null
    property Registry state_registry : null

    RegistryLookup {
        id: fx_chain_states_registry_lookup
        registry: root.state_registry
        key: 'fx_chain_states_registry'
    }
    property alias fx_chain_states_registry : fx_chain_states_registry_lookup.object

    readonly property string obj_id: initial_descriptor.id
    property string name: initial_descriptor.name

    // TODO: kind of a bad place to put this.
    // When the loop starts recording, ensure all channels have their
    // "recording started at" updated and also our FX chain state is
    // cached for wet channels.
    property var prev_mode: mode
    onModeChanged: {
        if (prev_mode != mode) {
            var now = (new Date()).toISOString()
            var fx_chain_desc_id = null
            channels.forEach(c => {
                if (ModeHelpers.is_recording_mode_for(mode, c.mode)) {
                    c.recording_started_at = now
                    if (c.mode == Types.ChannelMode.Wet && track_widget.maybe_fx_chain) {
                        if (!fx_chain_desc_id) {
                            fx_chain_desc_id = fx_chain_states_registry.generate_id('fx_chain_state')
                            var fx_chain_desc = track_widget.maybe_fx_chain.actual_session_descriptor()
                            delete fx_chain_desc.ports // Port descriptions not needed for state caching, this is track-dependent
                            fx_chain_desc.title = ""   // No title indicates elsewhere that this is not a snapshot that the user can directly see in the list.
                            fx_chain_desc.id = fx_chain_desc_id
                            fx_chain_states_registry.register (fx_chain_desc_id, fx_chain_desc)
                        }
                        c.recording_fx_chain_state_id = fx_chain_desc_id
                    }
                }
            })
        }
        prev_mode = mode
    }

    // The "loop volume" refers to the output volume from the wet
    // and/or direct channels. Volume of the dry channels is always
    // at 1.
    // Furthermore, for stereo signals we resolve the individual
    // channel volumes to a "volume + balance" combination for the
    // overall loop.
    readonly property real initial_volume: {
        var volumes = initial_descriptor.channels
            .filter(c => ['direct', 'wet'].includes(c.mode))
            .map(c => ('volume' in c) ? c.volume : undefined)
            .filter(c => c != undefined)
        return volumes.length > 0 ? Math.max(...volumes) : 1.0
    }
    readonly property bool is_stereo: initial_descriptor.channels.filter(c => ['direct', 'wet'].includes(c.mode)).length == 2
    readonly property var initial_stereo_balance : {
        if (!is_stereo) { return undefined; }
        var channels = initial_descriptor.channels.filter(c => ['direct', 'wet'].includes(c.mode) && c.type == "audio")
        channels.sort((a,b) => a.id.localeCompare(b.id))
        if (channels.length != 2) { throw new Error("Could not find stereo channels") }
        return Stereo.balance(channels[0].volume, channels[1].volume)
    }

    SchemaCheck {
        descriptor: root.initial_descriptor
        schema: 'loop.1'
    }

    function force_load_backend() {
        dynamic_loop.force_load = true
    }

    function actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to) {
        return {
            'schema': 'loop.1',
            'id': obj_id,
            'name': name,
            'length': maybe_loop.length,
            'is_master': is_master,
            'channels': channels.map((c) => c.actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to))
        }
    }
    function queue_load_tasks(data_files_dir, add_tasks_to) {
        var channels = channels
        var have_data_files = channels.map(c => c.has_data_file())
        if (have_data_files.filter(d => d == true).length > 0) {
            force_load_backend()
            channels.forEach((c) => c.queue_load_tasks(data_files_dir, add_tasks_to))
        }
    }

    readonly property var audio_channel_descriptors: initial_descriptor.channels.filter(c => c.type == 'audio')
    readonly property var midi_channel_descriptors: initial_descriptor.channels.filter(c => c.type == 'midi')
    property bool loaded : false

    RegistryLookup {
        id: master_loop_lookup
        registry: state_registry
        key: 'master_loop'
    }
    property alias master_loop : master_loop_lookup.object

    RegistryLookup {
        id: targeted_loop_lookup
        registry: state_registry
        key: 'targeted_loop'
    }
    property alias targeted_loop : targeted_loop_lookup.object
    property bool targeted : targeted_loop == root

    RegistryLookup {
        id: hovered_scene_loops_lookup
        registry: state_registry
        key: 'hovered_scene_loop_ids'
    }
    property alias hovered_scene_loop_ids : hovered_scene_loops_lookup.object
    property bool is_in_hovered_scene : hovered_scene_loop_ids && hovered_scene_loop_ids.has(obj_id)

    RegistryLookup {
        id: selected_scene_loops_lookup
        registry: state_registry
        key: 'selected_scene_loop_ids'
    }
    property alias selected_scene_loop_ids : selected_scene_loops_lookup.object
    property bool is_in_selected_scene : selected_scene_loop_ids && selected_scene_loop_ids.has(obj_id)

    RegistryLookup {
        id: scenes_widget_lookup
        registry: state_registry
        key: 'scenes_widget'
    }
    property alias scenes_widget : scenes_widget_lookup.object

    RegistryLookup {
        id: selected_loops_lookup
        registry: state_registry
        key: 'selected_loop_ids'
    }
    property alias selected_loop_ids : selected_loops_lookup.object
    property bool selected : selected_loop_ids ? selected_loop_ids.has(obj_id) : false

    function toggle_in_current_scene() {
        if (scenes_widget) {
            scenes_widget.toggle_loop_in_current_scene (obj_id)
        }
    }

    RegisterInRegistry {
        id: obj_reg_entry
        object: root
        key: obj_id
        registry: root.objects_registry
    }

    RegisterInRegistry {
        id: master_reg_entry
        enabled: initial_descriptor.is_master
        registry: root.state_registry
        key: 'master_loop'
        object: root
    }

    Component.onCompleted: {
        loaded = Qt.binding(function() { return maybe_loop.loaded} )
    }

    property var additional_context_menu_options : null // dict of option name -> functor

    // Internally controlled
    readonly property DynamicLoop maybe_loop : dynamic_loop
    readonly property Loop maybe_loaded_loop : dynamic_loop.maybe_loop
    readonly property bool is_master: master_loop && master_loop == this
    readonly property var delay_for_targeted : {
        // This property is used for synchronizing to the targeted loop.
        // If:
        // - A targeted loop is active, and
        // - The targeted loop is doing playback
        // then this property will hold the amount of master loop cycles
        // to delay in order to transition in sync with the targeted loop.
        // Otherwise, it will be 0.
        if (targeted_loop) {
            var to_transition = undefined
            if (targeted_loop.next_transition_delay >= 0 && targeted_loop.next_mode >= 0) {
                to_transition = targeted_loop.next_transition_delay
            }
            var to_end = undefined
            if (ModeHelpers.is_mode_with_predictable_end(targeted_loop.mode)) {
                to_end = Math.floor((targeted_loop.length - targeted_loop.position) / master_loop.length)
            }
            if (to_transition == undefined && to_end == undefined) { return undefined }
            else if (to_transition != undefined && to_end != undefined) { return Math.min(to_transition, to_end) }
            else if (to_transition != undefined) { return to_transition }
            else return to_end
        }
        return undefined
    }
    readonly property int use_delay : delay_for_targeted != undefined ? delay_for_targeted : 0

    property int n_multiples_of_master_length: master_loop ?
        Math.ceil(dynamic_loop.length / master_loop.maybe_loop.length) : 1
    property int current_cycle: master_loop ?
        Math.floor(dynamic_loop.position / master_loop.maybe_loop.length) : 0

    // Signals
    signal channelInitializedChanged()
    signal cycled

    // Methods
    function transition_loops(loops, mode, delay, wait_for_sync) {
        loops.forEach(loop => {
            // Force to load all loops involved
            loop.force_load_backend()
        })
        var backend_loops = loops.map(o => o.maybe_loaded_loop)
        backend_loops[0].transition_multiple(backend_loops, mode, delay, wait_for_sync)
    }
    function transition(mode, delay, wait_for_sync) {
        // Do the transition for this loop and all selected loops, if any
        var selected_ids = new Set(state_registry.maybe_get('selected_loop_ids', new Set()))
        selected_ids.add(obj_id)
        var objects = Array.from(selected_ids).map(id => objects_registry.maybe_get(id, undefined)).filter(v => v != undefined)
        transition_loops(objects, mode, delay, wait_for_sync)
    }
    function play_solo_in_track() {
        // Gather all selected loops
        var _selected_ids = new Set(state_registry.maybe_get('selected_loop_ids', new Set()))
        _selected_ids.add(obj_id)
        // Gather all other loops that are in the same track(s)
        var _selected_loops = []
        _selected_ids.forEach(id => _selected_loops.push(objects_registry.get(id)))
        var _all_track_loops = []
        _selected_loops.forEach(loop => {
            for(var i=0; i<loop.track_widget.loops.length; i++) {
                _all_track_loops.push(loop.track_widget.loops[i])
            }
        })
        var _other_loops = _all_track_loops.filter(l => !_selected_loops.includes(l))
        // Do the transitions
        transition_loops(_other_loops, Types.LoopMode.Stopped, use_delay, root.sync_active)
        transition_loops(_selected_loops, Types.LoopMode.Playing, use_delay, root.sync_active)
    }
    function clear(length, emit=true) {
        dynamic_loop.clear(length);
    }
    function qml_close() {
        obj_reg_entry.close()
        master_reg_entry.close()
        for(var i=0; i<audio_channels.model; i++) {
            audio_channels.itemAt(i).qml_close();
        }
        for(var i=0; i<midi_channels.model; i++) {
            midi_channels.itemAt(i).qml_close();
        }
        dynamic_loop.qml_close();
    }

    function select(clear = false) {
        untarget()
        if (key_modifiers.control_pressed) {
            state_registry.add_to_set('selected_loop_ids', obj_id)
        } else {
            state_registry.replace('selected_loop_ids', new Set([obj_id]))
        }
    }
    function deselect() {
        if (key_modifiers.control_pressed) {
            state_registry.remove_from_set('selected_loop_ids', obj_id)
        } else {
            state_registry.replace('selected_loop_ids', new Set())
        }
    }
    function target() {
        deselect()
        state_registry.replace('targeted_loop', root)
    }
    function untarget() {
        if (targeted) {
            state_registry.replace('targeted_loop', null)
        }
    }
    function toggle_selected(clear_if_select = false) {
        if (selected) { deselect() } else { select(clear_if_select) }
    }
    function toggle_targeted() {
        if (targeted) { untarget() } else { target() }
    }

    function get_audio_output_channels() {
        var chans = Array.from(Array(audio_channels.model).keys()).map((i) => audio_channels.itemAt(i))
        return chans.filter(c => c && c.obj_id.match(/.*_(?:wet|direct)(?:_[0-9]+)?$/))
    }

    function get_stereo_audio_output_channels() {
        var chans = get_audio_output_channels()
        if (chans.length != 2) { throw new Error("attempting to get stereo channels, no stereo found") }
        chans.sort((a, b) => a.obj_id.localeCompare(b.obj_id))
        return chans
    }

    property real last_pushed_volume: initial_volume
    property real last_pushed_stereo_balance: initial_stereo_balance ? initial_stereo_balance : 0.0

    function push_volume(volume) {
        // Only set the volume on audio output channels:
        // - Volume not supported on MIDI
        // - Send should always have the original recorded volume of the dry signal.
        // Also, volume + balance together make up the channel volumes in stereo mode.
        if (root.is_stereo) {
            var lr = get_stereo_audio_output_channels()
            var volumes = Stereo.individual_volumes(volume, last_pushed_stereo_balance)
            lr[0].set_volume(volumes[0])
            lr[1].set_volume(volumes[1])
        } else {
            get_audio_output_channels().forEach(c => c.set_volume(volume))
        }

        last_pushed_volume = volume
    }

    function push_stereo_balance(balance) {
        if (root.is_stereo) {
            var lr = get_stereo_audio_output_channels()
            var volumes = Stereo.individual_volumes(last_pushed_volume, balance)
            lr[0].set_volume(volumes[0])
            lr[1].set_volume(volumes[1])
            last_pushed_stereo_balance = balance
        }
    }

    function set_length(length) {
        if (maybe_loaded_loop) {
            maybe_loaded_loop.set_length(length)
        }
    }

    width: childrenRect.width
    height: childrenRect.height
    clip: true

    property alias length : dynamic_loop.length
    property alias position : dynamic_loop.position
    property alias mode : dynamic_loop.mode
    property alias next_mode : dynamic_loop.next_mode
    property alias next_transition_delay : dynamic_loop.next_transition_delay
    DynamicLoop {
        id: dynamic_loop
        force_load : is_master // Master loop should always be there to sync to
        sync_source : root.master_loop && !root.is_master ? root.master_loop.maybe_loaded_loop : null;
        onBackendLoopLoaded: set_length(initial_descriptor.length)
        onCycled: root.cycled()

        Repeater {
            id: audio_channels
            model : root.audio_channel_descriptors.length

            LoopAudioChannel {
                loop: dynamic_loop.maybe_loop
                descriptor: root.audio_channel_descriptors[index]
                objects_registry: root.objects_registry
                state_registry: root.state_registry
                onRequestBackendInit: root.force_load_backend()
                onInitializedChanged: root.channelInitializedChanged()
            }
        }
        Repeater {
            id: midi_channels
            model : root.midi_channel_descriptors.length

            LoopMidiChannel {
                loop: dynamic_loop.maybe_loop
                descriptor: root.midi_channel_descriptors[index]
                objects_registry: root.objects_registry
                state_registry: root.state_registry
                onRequestBackendInit: root.force_load_backend()
                onInitializedChanged: root.channelInitializedChanged()
            }
        }
    }

    RegistryLookups {
        keys: root.initial_descriptor ? root.initial_descriptor.channels.map(c => c.id) : []
        registry: root.objects_registry
        id: lookup_channels
    }
    property alias channels: lookup_channels.objects
    property var audio_channels: channels.filter(c => c.descriptor.type == 'audio')
    property var midi_channels: channels.filter(c => c.descriptor.type == 'midi')

    RegistryLookup {
        id: lookup_sync_active
        registry: root.state_registry
        key: 'sync_active'
    }
    property alias sync_active: lookup_sync_active.object

    // UI
    StatusRect {
        id: statusrect
        loop: root.maybe_loop

        LoopDetailsWindow {
            id: detailswindow
            title: root.initial_descriptor.id + " details"
            loop: root
            master_loop: root.master_loop
        }

        ContextMenu {
            id: contextmenu
        }
    }

    component StatusRect : Rectangle {
        id: statusrect
        property var loop
        property bool hovered : area.containsMouse
        property string name : root.name

        signal propagateMousePosition(var point)
        signal propagateMouseExited()

        width: loopitem.width
        height: loopitem.height

        color: (loop.length > 0) ? '#000044' : Material.background

        border.color: {
            var default_color = 'grey'
            if (!statusrect.loop) {
                return default_color;
            }

            if (root.targeted) {
                return "orange";
            } if (root.selected) {
                return 'yellow';
            } else if (root.is_in_hovered_scene) {
                return 'blue';
            } else if (root.is_in_selected_scene) {
                return 'red';
            }

            if (statusrect.loop.length == 0) {
                return default_color;
            }

            return '#DDDDDD' //blue'
        }
        border.width: 2

        Item {
            anchors.fill: parent
            anchors.margins: 2

            LoopProgressRect {
                anchors.fill: parent
                loop: statusrect.loop
            }
        }

        ProgressBar {
            id: peak_meter_l
            visible: root.is_stereo
            anchors {
                left: parent.left
                right: parent.horizontalCenter
                bottom: parent.bottom
                margins: 2
            }
            height: 3

            AudioLevelMeterModel {
                id: output_peak_meter_l
                max_dt: 0.1
                input: root.maybe_loop.display_peaks.length >= 1 ? root.maybe_loop.display_peaks[0] : 0.0
            }

            from: -50.0
            to: 0.0
            value: output_peak_meter_l.value

            background: Item { anchors.fill: peak_meter_l }
            contentItem: Item {
                implicitWidth: peak_meter_l.width
                implicitHeight: peak_meter_l.height

                Rectangle {
                    width: peak_meter_l.visualPosition * peak_meter_l.width
                    height: peak_meter_l.height
                    color: Material.accent
                    x: peak_meter_l.width - width
                }
            }
        }

        ProgressBar {
            id: peak_meter_r
            visible: root.is_stereo
            anchors {
                left: parent.horizontalCenter
                right: parent.right
                bottom: parent.bottom
                margins: 2
            }
            height: 3

            AudioLevelMeterModel {
                id: output_peak_meter_r
                max_dt: 0.1
                input: root.maybe_loop.display_peaks.length >= 2 ? root.maybe_loop.display_peaks[1] : 0.0
            }

            from: -50.0
            to: 0.0
            value: output_peak_meter_r.value

            background: Item { anchors.fill: peak_meter_r }
            contentItem: Item {
                implicitWidth: peak_meter_r.width
                implicitHeight: peak_meter_r.height

                Rectangle {
                    width: peak_meter_r.visualPosition * peak_meter_r.width
                    height: peak_meter_r.height
                    color: Material.accent
                }
            }
        }

        ProgressBar {
            id: peak_meter_overall
            visible: !root.is_stereo
            anchors {
                left: peak_meter_l.left
                right: peak_meter_r.right
                bottom: peak_meter_l.bottom
                top: peak_meter_l.top
            }
            height: 3

            AudioLevelMeterModel {
                id: output_peak_meter_overall
                max_dt: 0.1
                input: root.maybe_loop.display_peaks.length > 0 ? Math.max(...root.maybe_loop.display_peaks) : 0.0
            }

            from: -30.0
            to: 0.0
            value: output_peak_meter_overall.value

            background: Item { anchors.fill: peak_meter_overall }
            contentItem: Item {
                implicitWidth: peak_meter_overall.width
                implicitHeight: peak_meter_overall.height

                Rectangle {
                    width: peak_meter_overall.visualPosition * peak_meter_overall.width
                    height: peak_meter_overall.height
                    color: Material.accent
                }
            }
        }

        MouseArea {
            id: area
            x: 0
            y: 0
            anchors.fill: parent
            hoverEnabled: true
            propagateComposedEvents: true
            onPositionChanged: (mouse) => { statusrect.propagateMousePosition(mapToGlobal(mouse.x, mouse.y)) }
            onExited: statusrect.propagateMouseExited()
        }

        MaterialDesignIcon {
            size: 10
            name: 'star'
            color: "yellow"

            anchors {
                left: parent.left
                leftMargin: 2
                top: parent.top
                topMargin: 2
            }

            visible: root.is_master
        }

        Item {
            id: loopitem
            width: 100
            height: 26
            x: 0
            y: 0

            Item {
                id: iconitem
                width: 24
                height: 24
                y: 0
                x: 0

                property bool show_next_mode: 
                    statusrect.loop &&
                        statusrect.loop.next_mode != null &&
                        statusrect.loop.mode != statusrect.loop.next_mode

                LoopStateIcon {
                    id: loopstateicon
                    mode: statusrect.loop ? statusrect.loop.mode : Types.LoopMode.Unknown
                    show_timer_instead: parent.show_next_mode
                    visible: !parent.show_next_mode || (parent.show_next_mode && statusrect.loop.next_transition_delay == 0)
                    connected: true
                    size: iconitem.height
                    y: 0
                    anchors.horizontalCenter: iconitem.horizontalCenter
                    muted: false
                    //muted: { console.log('unimplemented muted'); return false; }
                    empty: statusrect.loop.length == 0
                    onDoubleClicked: (event) => {
                            if (event.button === Qt.LeftButton) { root.target() }
                        }
                    onClicked: (event) => {
                            if (event.button === Qt.LeftButton) { 
                                if (root.targeted) { root.untarget(); root.deselect() }
                                else { root.toggle_selected(!key_modifiers.control_pressed) }
                            }
                            else if (event.button === Qt.MiddleButton) { root.toggle_in_current_scene() }
                            else if (event.button === Qt.RightButton) { contextmenu.popup() }
                        }
                }
                LoopStateIcon {
                    id: loopnextstateicon
                    mode: parent.show_next_mode ?
                        statusrect.loop.next_mode : Types.LoopMode.Unknown
                    show_timer_instead: false
                    connected: true
                    size: iconitem.height * 0.65
                    y: 0
                    anchors.right : loopstateicon.right
                    anchors.bottom : loopstateicon.bottom
                    visible: parent.show_next_mode
                }
                Text {
                    text: statusrect.loop ? (statusrect.loop.next_transition_delay + 1).toString(): ''
                    visible: parent.show_next_mode && statusrect.loop.next_transition_delay > 0
                    anchors.fill: loopstateicon
                    color: Material.foreground
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    font.bold: true
                }
            }

            Text {
                text: statusrect.name
                color: /\([0-9]+\)/.test(statusrect.name) ? 'grey' : Material.foreground
                font.pixelSize: 11
                verticalAlignment: Text.AlignVCenter
                horizontalAlignment: Text.AlignLeft
                anchors.left: iconitem.right
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                anchors.right: parent.right
                anchors.rightMargin: 6
                visible: !buttongrid.visible
            }

            Grid {
                visible: statusrect.hovered || playlivefx.hovered || playsolointrack.hovered || recordN.hovered || recordfx.hovered || recordn_menu.visible
                x: 20
                y: 2
                columns: 4
                id: buttongrid
                property int button_width: 18
                property int button_height: 22
                spacing: 1

                SmallButtonWithCustomHover {
                    id : play
                    width: buttongrid.button_width
                    height: buttongrid.button_height
                    IconWithText {
                        size: parent.width
                        anchors.centerIn: parent
                        name: 'play'
                        color: 'green'
                        text_color: Material.foreground
                        text: root.delay_for_targeted != undefined ? ">" : ""
                    }

                    onClicked: root.transition(Types.LoopMode.Playing, root.use_delay, root.sync_active)

                    ToolTip.delay: 1000
                    ToolTip.timeout: 5000
                    ToolTip.visible: hovered
                    ToolTip.text: "Play wet recording."

                    Connections {
                        target: statusrect
                        function onPropagateMousePosition(pt) { play.onMousePosition(pt) }
                        function onPropagateMouseExited() { play.onMouseExited() }
                    }

                    Popup {
                        background: Item{}
                        visible: play.hovered || ma.containsMouse
                        leftInset: 0
                        rightInset: 0
                        topInset: 0
                        bottomInset: 0
                        padding: 0
                        margins: 0

                        x: 0
                        y: play.height

                        Rectangle {
                            width: playlivefx.width
                            height: playsolointrack.height + playlivefx.height
                            color: statusrect.color

                            MouseArea {
                                id: ma
                                x: 0
                                y: 0
                                width: parent.width
                                height: parent.height
                                hoverEnabled: true

                                onPositionChanged: (mouse) => { 
                                    var p = mapToGlobal(mouse.x, mouse.y)
                                    playsolointrack.onMousePosition(p)
                                    playlivefx.onMousePosition(p)
                                }
                                onExited: { playlivefx.onMouseExited(); playsolointrack.onMouseExited() }
                            }

                            Column {
                                SmallButtonWithCustomHover {
                                    id : playsolointrack
                                    width: buttongrid.button_width
                                    height: buttongrid.button_height
                                    IconWithText {
                                        size: parent.width
                                        anchors.centerIn: parent
                                        name: 'play'
                                        color: 'green'
                                        text_color: Material.foreground
                                        text: root.delay_for_targeted != undefined ? ">S" : "S"
                                    }
                                    onClicked: { if(statusrect.loop) {
                                        root.play_solo_in_track()
                                        }}

                                    ToolTip.delay: 1000
                                    ToolTip.timeout: 5000
                                    ToolTip.visible: hovered
                                    ToolTip.text: "Play wet recording solo in track."
                                }
                                SmallButtonWithCustomHover {
                                    id : playlivefx
                                    width: buttongrid.button_width
                                    height: buttongrid.button_height
                                    IconWithText {
                                        size: parent.width
                                        anchors.centerIn: parent
                                        name: 'play'
                                        color: 'orange'
                                        text_color: Material.foreground
                                        text: root.delay_for_targeted != undefined ? ">" : ""
                                    }
                                    onClicked: root.transition(Types.LoopMode.PlayingDryThroughWet, root.use_delay, root.sync_active)

                                    ToolTip.delay: 1000
                                    ToolTip.timeout: 5000
                                    ToolTip.visible: hovered
                                    ToolTip.text: "Play dry recording through live effects. Allows hearing FX changes on-the-fly."
                                }
                            }
                        }
                    }
                }

                SmallButtonWithCustomHover {
                    id : record
                    width: buttongrid.button_width
                    height: buttongrid.button_height
                    IconWithText {
                        size: parent.width
                        anchors.centerIn: parent
                        name: 'record'
                        color: 'red'
                        text_color: Material.foreground
                        text: root.delay_for_targeted != undefined ? ">" : ""
                    }

                    onClicked: root.transition(Types.LoopMode.Recording, root.use_delay, root.sync_active)

                    ToolTip.delay: 1000
                    ToolTip.timeout: 5000
                    ToolTip.visible: hovered
                    ToolTip.text: "Trigger/stop recording. For master loop, starts immediately. For others, start/stop synced to next master loop cycle."

                    Connections {
                        target: statusrect
                        function onPropagateMousePosition(pt) { record.onMousePosition(pt) }
                        function onPropagateMouseExited() { record.onMouseExited() }
                    }

                    Popup {
                        background: Item{}
                        visible: record.hovered || ma_.containsMouse
                        leftInset: 0
                        rightInset: 0
                        topInset: 0
                        bottomInset: 0
                        padding: 0
                        margins: 0

                        x: 0
                        y: play.height

                        Rectangle {
                            width: recordN.width
                            height: recordN.height + recordfx.height
                            color: statusrect.color

                            MouseArea {
                                id: ma_
                                x: 0
                                y: 0
                                width: parent.width
                                height: parent.height
                                hoverEnabled: true

                                onPositionChanged: (mouse) => { 
                                    var p = mapToGlobal(mouse.x, mouse.y)
                                    recordN.onMousePosition(p)
                                    recordfx.onMousePosition(p)
                                }
                                onExited: { recordN.onMouseExited(); recordfx.onMouseExited() }
                            }

                            Column {
                                SmallButtonWithCustomHover {
                                    id : recordN
                                    property int n: 1
                                    width: buttongrid.button_width
                                    height: buttongrid.button_height
                                    IconWithText {
                                        size: parent.width
                                        anchors.centerIn: parent
                                        name: 'record'
                                        color: 'red'
                                        text_color: Material.foreground
                                        text: (root.targeted_loop !== undefined && root.targeted_loop !== null) ? "><" : recordN.n.toString()
                                        font.pixelSize: size / 2.0
                                    }

                                    function execute(delay, n_cycles) {
                                        root.transition(Types.LoopMode.Recording, delay, true)
                                        root.transition(Types.LoopMode.Playing, delay + n_cycles, true)
                                    }

                                    onClicked: {
                                        if (root.targeted_loop === undefined || root.targeted_loop === null) {
                                            execute(0, recordN.n)
                                        } else {
                                            // A target loop is set. Do the "record together with" functionality.
                                            // TODO: code is duplicated in app shared state for MIDI source
                                            var n_cycles_delay = 0
                                            var n_cycles_record = 1
                                            n_cycles_record = Math.ceil(root.targeted_loop.length / root.master_loop.length)
                                            if (ModeHelpers.is_playing_mode(root.targeted_loop.mode)) {
                                                var current_cycle = Math.floor(root.targeted_loop.position / root.master_loop.length)
                                                n_cycles_delay = Math.max(0, n_cycles_record - current_cycle - 1)
                                            }
                                            execute(n_cycles_delay, n_cycles_record)
                                        }
                                    }
                                    onPressAndHold: { recordn_menu.popup() }

                                    ToolTip.delay: 1000
                                    ToolTip.timeout: 5000
                                    ToolTip.visible: hovered
                                    ToolTip.text: "Trigger fixed-length recording (usual) or 'record with' (if a target loop is set). 'Record with' will record for one full iteration synced with the target loop. Otherwise, fixed length (number shown) is the amount of master loop cycles to record. Press and hold this button to change this number."

                                    // TODO: editable text box instead of fixed options
                                    Menu {
                                        id: recordn_menu
                                        title: 'Select # of cycles'
                                        MenuItem {
                                            text: "1 cycle"
                                            onClicked: () => { recordN.execute(0, 1) }
                                        }
                                        MenuItem {
                                            text: "2 cycles"
                                            onClicked: () => { recordN.execute(0, 2) }
                                        }
                                        MenuItem {
                                            text: "3 cycles"
                                            onClicked: () => { recordN.execute(0, 3) }
                                        }
                                        MenuItem {
                                            text: "4 cycles"
                                            onClicked: () => { recordN.execute(0, 4) }
                                        }
                                        MenuItem {
                                            text: "6 cycles"
                                            onClicked: () => { recordN.execute(0, 6) }
                                        }
                                        MenuItem {
                                            text: "8 cycles"
                                            onClicked: () => { recordN.execute(0, 8) }
                                        }
                                        MenuItem {
                                            text: "16 cycles"
                                            onClicked: () => { recordN.execute(0, 16) }
                                        }
                                    }
                                }
                            
                                SmallButtonWithCustomHover {
                                    id : recordfx
                                    width: buttongrid.button_width
                                    height: buttongrid.button_height
                                    IconWithText {
                                        size: parent.width
                                        anchors.centerIn: parent
                                        name: 'record'
                                        color: 'orange'
                                        text_color: Material.foreground
                                        text: root.delay_for_targeted != undefined ? ">" : ""
                                    }
                                    onClicked: {
                                        var n = root.n_multiples_of_master_length
                                        var delay = 
                                            root.delay_for_targeted != undefined ? 
                                                root.use_delay : // delay to other
                                                root.n_multiples_of_master_length - root.current_cycle - 1 // delay to self
                                        var prev_mode = statusrect.loop.mode
                                        root.transition(Types.LoopMode.RecordingDryIntoWet, delay, true)
                                        statusrect.loop.transition(prev_mode, delay + n, true)
                                    }

                                    ToolTip.delay: 1000
                                    ToolTip.timeout: 5000
                                    ToolTip.visible: hovered
                                    ToolTip.text: "Trigger FX re-record. This will play the full dry loop once with live FX, recording the result for wet playback."
                                }
                            }
                        }
                    }
                }

                SmallButtonWithCustomHover {
                    id : stop
                    width: buttongrid.button_width
                    height: buttongrid.button_height
                    IconWithText {
                        size: parent.width
                        anchors.centerIn: parent
                        name: 'stop'
                        color: Material.foreground
                        text_color: Material.foreground
                        text: root.delay_for_targeted != undefined ? ">" : ""
                    }

                    onClicked: root.transition(Types.LoopMode.Stopped, root.use_delay, root.sync_active)

                    ToolTip.delay: 1000
                    ToolTip.timeout: 5000
                    ToolTip.visible: hovered
                    ToolTip.text: "Stop."

                    Connections {
                        target: statusrect
                        function onPropagateMousePosition(pt) { stop.onMousePosition(pt) }
                        function onPropagateMouseExited() { stop.onMouseExited() }
                    }
                }
            }

            Item {
                anchors.right: parent.right
                width: 18
                height: 18
                anchors.verticalCenter: parent.verticalCenter
                anchors.rightMargin: 5

                // Display the volume dial always
                AudioDial {
                    id: volume_dial
                    anchors.fill: parent
                    from: -30.0
                    to:   20.0
                    value: 0.0
                    property real initial_linear_value: root.initial_volume
                    onInitial_linear_valueChanged: set_from_linear(initial_linear_value)
                    Component.onCompleted: set_from_linear(initial_linear_value)

                    LinearDbConversion {
                        dB_threshold: parent.from
                        id: convert_volume
                    }

                    show_value_tooltip: true
                    value_tooltip_postfix: ' dB'

                    onMoved: {
                        convert_volume.dB = volume_dial.value
                        push_volume(convert_volume.linear)
                    }
                    function set_from_linear(val) {
                        convert_volume.linear = val
                        value = convert_volume.dB
                    }

                    handle.width: 4
                    handle.height: 4
                    
                    background: Rectangle {
                        radius: width / 2.0
                        width: parent.width
                        color: '#222222'
                        border.width: 1
                        border.color: 'grey'
                    }

                    label: 'V'
                    tooltip: 'Loop volume. Double-click to reset.'

                    HoverHandler {
                        id: volume_dial_hover
                    }
                }

                Popup {
                    property bool visible_in: root.is_stereo && (volume_dial_hover.hovered || volume_dial.pressed || balance_dial_hover.hovered)
                    Timer {
                        id: visible_off_timer
                        interval: 50
                        repeat: false
                    }
                    onVisible_inChanged: { if (!visible_in) { visible_off_timer.restart(); } }
                    visible: visible_in || visible_off_timer.running
                    background: Rectangle { 
                        width: balance_dial.width
                        height: balance_dial.height
                        color: Material.background
                    }
                    leftInset: 0
                    rightInset: 0
                    topInset: 0
                    bottomInset: 0
                    padding: 0
                    margins: 0
                    x: parent.width + 4
                    y: 0

                    AudioDial {
                        id: balance_dial
                        from: -1.0
                        to:   1.0
                        value: root.is_stereo ? root.initial_stereo_balance : 0.0

                        width: volume_dial.width
                        height: volume_dial.height

                        onMoved: {
                            push_stereo_balance(value)
                        }

                        show_value_tooltip: true

                        handle.width: 4
                        handle.height: 4
                        
                        background: Rectangle {
                            radius: width / 2.0
                            width: parent.width
                            color: '#222222'
                            border.width: 1
                            border.color: 'grey'
                        }

                        HoverHandler {
                            id: balance_dial_hover
                        }

                        label: 'B'
                        tooltip: 'Loop stereo balance. Double-click to reset.'
                    }
                }
            }
        }
    }

    component LoopProgressRect : Item {
        id: loopprogressrect
        property var loop

        Rectangle {
            function getRightMargin() {
                var st = loopprogressrect.loop
                if(st && st.length && st.length > 0) {
                    return st.mode == Types.LoopMode.Recording ?
                        0.0 :
                        (1.0 - (st.position / st.length)) * parent.width
                }
                return parent.width
            }

            anchors {
                fill: parent
                rightMargin: getRightMargin()
            }
            color: {
                var default_color = '#444444'
                if (!loopprogressrect.loop) {
                    return default_color;
                }

                switch(loopprogressrect.loop.mode) {
                case Types.LoopMode.Playing:
                    return '#004400';
                case Types.LoopMode.PlayingLiveFX:
                    return '#333300';
                case Types.LoopMode.Recording:
                    return '#660000';
                case Types.LoopMode.RecordingFX:
                    return '#663300';
                default:
                    return default_color;
                }
            }
        }

        Rectangle {
            visible: root.maybe_loop.display_midi_notes_active > 0
            anchors {
                right: parent.right
                top: parent.top
                bottom: parent.bottom
            }
            width: 8

            color: '#00BBFF'
        }

        Rectangle {
            visible: root.maybe_loop.display_midi_events_triggered > 0
            anchors {
                right: parent.right
                top: parent.top
                bottom: parent.bottom
            }
            width: 8

            color: '#00FFFF'
        }
    }

    component LoopStateIcon : Item {
        id: lsicon
        property int mode
        property bool connected
        property bool show_timer_instead
        property bool empty
        property bool muted
        property int size
        property string description: ModeHelpers.mode_name(mode)

        signal clicked(var mouse)
        signal doubleClicked(var mouse)

        width: size
        height: size

        IconWithText {
            id: main_icon
            anchors.fill: parent

            name: {
                if(!lsicon.connected) {
                    return 'cancel'
                }
                if(lsicon.show_timer_instead) {
                    return 'timer-sand'
                }
                if(lsicon.empty) {
                    return 'border-none-variant'
                }

                switch(lsicon.mode) {
                case Types.LoopMode.Playing:
                case Types.LoopMode.PlayingDryThroughWet:
                    return lsicon.muted ? 'volume-mute' : 'play'
                case Types.LoopMode.Recording:
                case Types.LoopMode.RecordingDryIntoWet:
                    return 'record-rec'
                case Types.LoopMode.Stopped:
                    return 'stop'
                default:
                    return 'help-circle'
                }
            }

            color: {
                if(!lsicon.connected) {
                    return 'grey'
                }
                if(lsicon.show_timer_instead) {
                    return Material.foreground
                }
                switch(lsicon.mode) {
                case Types.LoopMode.Playing:
                    return '#00AA00'
                case Types.LoopMode.Recording:
                    return 'red'
                case Types.LoopMode.RecordingDryIntoWet:
                case Types.LoopMode.PlayingDryThroughWet:
                    return 'orange'
                default:
                    return 'grey'
                }
            }

            text_color: Material.foreground
            text: {
                if(lsicon.show_timer_instead) {
                    return ''
                }
                switch(lsicon.mode) {
                case Types.LoopMode.RecordingDryIntoWet:
                case Types.LoopMode.PlayingDryThroughWet:
                    return 'FX'
                default:
                    return ''
                }
            }

            size: parent.size
        }

        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            propagateComposedEvents: true
            acceptedButtons: Qt.RightButton | Qt.MiddleButton | Qt.LeftButton
            onClicked: (mouse) => lsicon.clicked(mouse)
            onDoubleClicked: (mouse) => lsicon.doubleClicked(mouse)
            id: ma
        }

        ToolTip {
            delay: 1000
            visible: ma.containsMouse
            text: description
        }
    }

    component ContextMenu: Item {
        property alias opened: menu.opened
        visible: menu.visible

        ClickTrackDialog {
            id: clicktrackdialog
            loop: root
            parent: Overlay.overlay
            x: (parent.width-width) / 2
            y: (parent.height-height) / 2

            onAcceptedClickTrack: (filename) => {
                                    loadoptionsdialog.filename = filename
                                    close()
                                    root.force_load_backend()
                                    loadoptionsdialog.update()
                                    loadoptionsdialog.open()
                                  }
        }

        Menu {
            id: menu
            title: 'Record'
            property int n_audio_channels: 0
            property int n_midi_channels: 0

            onAboutToShow: {
                n_audio_channels = root.audio_channels.length
                n_midi_channels = root.midi_channels.length
            }

            MenuItem {
                Row {
                    anchors.fill: parent
                    spacing: 5
                    anchors.leftMargin: 10
                    Text {
                        text: "Name:"
                        color: Material.foreground
                        anchors.verticalCenter: name_field.verticalCenter
                    }
                    TextField {
                        id: name_field
                        text: root.name
                        font.pixelSize: 12
                        onEditingFinished: {
                            root.name = text
                            background_focus.forceActiveFocus();
                        }
                    }
                }
            }
            MenuItem {
                text: "Loop details window"
                onClicked: () => { detailswindow.visible = true }
            }
            MenuItem {
               text: "Generate click loop..."
               onClicked: () => clicktrackdialog.open()
            }
            MenuItem {
                text: "Save audio..."
                onClicked: presavedialog.open()
                enabled: menu.n_audio_channels > 0
            }
            MenuItem {
                text: "Load audio..."
                onClicked: loaddialog.open()
                enabled: menu.n_audio_channels > 0
            }
            MenuItem {
                text: "Load MIDI..."
                enabled: menu.n_midi_channels > 0
                onClicked: {
                    var chans = root.midi_channels
                    if (chans.length == 0) { throw new Error("No MIDI channels to load"); }
                    if (chans.length > 1) { throw new Error("Cannot load into more than 1 MIDI channel"); }
                    midiloadoptionsdialog.channel = chans[0]
                    midiloaddialog.open()
                }
            }
            MenuItem {
                text: "Save MIDI..."
                enabled: menu.n_midi_channels > 0
                onClicked: {
                    var chans = root.midi_channels
                    if (chans.length == 0) { throw new Error("No MIDI channels to save"); }
                    if (chans.length > 1) { throw new Error("Cannot save more than 1 MIDI channel"); }
                    midisavedialog.channel = chans[0]
                    midisavedialog.open()
                }
            }
            MenuItem {
                text: "Push Master Loop Length"
                onClicked: {
                    if (master_loop) { master_loop.set_length(root.length) }
                }
            }
            MenuItem {
                text: "Clear"
                onClicked: () => {
                    if (root.maybe_loop) {
                        root.maybe_loop.clear(0);
                    }
                }
            }
            MenuItem {
                id: restore_fx_state_button

                property var cached_fx_state: {
                    if (!root.track_widget || !root.track_widget.maybe_fx_chain) { return undefined; }
                    var channel_states = root.channels
                        .filter(c => c.recording_fx_chain_state_id != undefined)
                        .map(c => c.recording_fx_chain_state_id);
                    return channel_states.length > 0 ? 
                        root.fx_chain_states_registry.maybe_get(channel_states[0], undefined)
                        : undefined
                }

                text: "Restore Recording FX State"
                enabled: cached_fx_state ? true : false
                onClicked: root.track_widget.maybe_fx_chain.restore_state(cached_fx_state.internal_state)
            }
        }

        Dialog {
            id: presavedialog
            standardButtons: Dialog.Save | Dialog.Cancel
            parent: Overlay.overlay
            x: (parent.width - width) / 2
            y: (parent.height - height) / 2
            modal: true
            readonly property int n_channels: channels.length
            property var channels: []

            width: 300
            height: 200

            function update() {
                var chans = root.audio_channels
                channels = chans.filter(select_channels.currentValue)
                footer.standardButton(Dialog.Save).enabled = n_channels > 0;
                savedialog.channels = channels
            }

            onAccepted: { close(); savedialog.open() }

            Column {
                Row {
                    Label {
                        text: "Include channels:"
                        anchors.verticalCenter: select_channels.verticalCenter
                    }
                    ComboBox {
                        id: select_channels
                        textRole: "text"
                        valueRole: "value"
                        model: [
                            { value: (chan) => true, text: "All" },
                            { value: (chan) => chan.mode == Types.ChannelMode.Direct, text: "Regular" },
                            { value: (chan) => chan.mode == Types.ChannelMode.Dry, text: "Dry" },
                            { value: (chan) => chan.mode == Types.ChannelMode.Wet, text: "Wet" }
                        ]
                        Component.onCompleted: presavedialog.update()
                        onActivated: presavedialog.update()
                    }
                }
                Label {
                    text: {
                        switch(presavedialog.n_channels) {
                            case 0:
                                return 'No channels match the selected filter.'
                            case 1:
                                return 'Format: mono audio'
                            case 2:
                                return 'Format: stereo audio'
                            default:
                                return 'Format: ' + presavedialog.n_channels.toString() + '-channel audio'
                        }
                    }
                }
            }
        }

        FileDialog {
            id: savedialog
            fileMode: FileDialog.SaveFile
            acceptLabel: 'Save'
            nameFilters: Object.entries(file_io.get_soundfile_formats()).map((e) => {
                var extension = e[0]
                var description = e[1].replace('(', '- ').replace(')', '')
                return description + ' (*.' + extension + ')';
            })
            property var channels: []
            onAccepted: {
                if (!root.maybe_loaded_loop) { 
                    console.log("Cannot save: loop not loaded")
                    return;
                }
                close()
                root.state_registry.save_action_started()
                try {
                    var filename = selectedFile.toString().replace('file://', '')
                    var samplerate = root.maybe_loaded_loop.backend.get_sample_rate()
                    var task = file_io.save_channels_to_soundfile_async(filename, samplerate, channels)
                    task.when_finished(() => root.state_registry.save_action_finished())
                } catch (e) {
                    root.state_registry.save_action_finished()
                    throw e;
                }
            }
        }

        FileDialog {
            id: midisavedialog
            fileMode: FileDialog.SaveFile
            acceptLabel: 'Save'
            nameFilters: ["MIDI files (*.mid)"]
            property var channel: null
            onAccepted: {
                if (!root.maybe_loaded_loop) { 
                    console.log("Cannot save: loop not loaded")
                    return;
                }
                close()
                var filename = selectedFile.toString().replace('file://', '')
                var samplerate = root.maybe_loaded_loop.backend.get_sample_rate()
                file_io.save_channel_to_midi_async(filename, samplerate, channel)
            }
        }

        FileDialog {
            id: loaddialog
            fileMode: FileDialog.OpenFile
            acceptLabel: 'Load'
            nameFilters: [
                'Supported sound files ('
                + Object.entries(file_io.get_soundfile_formats()).map((e) => '*.' + e[0].toLowerCase()).join(' ')
                + ')'
            ]
            onAccepted: {
                loadoptionsdialog.filename = selectedFile.toString().replace('file://', '')
                close()
                root.force_load_backend()
                loadoptionsdialog.update()
                loadoptionsdialog.open()
            }
        }

        Dialog {
            id: loadoptionsdialog
            standardButtons: Dialog.Open | Dialog.Cancel
            parent: Overlay.overlay
            x: (parent.width - width) / 2
            y: (parent.height - height) / 2
            modal: true
            property string filename: ''
            property var channels_to_load : []
            property var direct_audio_channels : []
            property var dry_audio_channels : []
            property var wet_audio_channels : []
            readonly property int n_channels : channels_to_load.length
            property int n_file_channels : 0
            property int file_sample_rate : 0
            property int backend_sample_rate : root.maybe_loaded_loop ? root.maybe_loaded_loop.backend.get_sample_rate() : 0
            property bool will_resample : file_sample_rate != backend_sample_rate

            width: 300
            height: 400

            onFilenameChanged: {
                var props = file_io.get_soundfile_info(filename)
                n_file_channels = props['channels']
                file_sample_rate = props['samplerate']
            }

            function update() {
                var chans = root.audio_channels
                direct_audio_channels = chans.filter(c => c.mode == Types.ChannelMode.Direct)
                dry_audio_channels = chans.filter(c => c.mode == Types.ChannelMode.Dry)
                wet_audio_channels = chans.filter(c => c.mode == Types.ChannelMode.Wet)
                var to_load = []
                if (direct_load_checkbox.checked) { to_load = to_load.concat(direct_audio_channels) }
                if (dry_load_checkbox.checked) { to_load = to_load.concat(dry_audio_channels) }
                if (wet_load_checkbox.checked) { to_load = to_load.concat(wet_audio_channels) }
                channels_to_load = to_load
                footer.standardButton(Dialog.Open).enabled = channels_to_load.length > 0;
            }

            // TODO: there has to be a better way
            Timer {                                                                                                                                                                                 
                interval: 100
                running: loadoptionsdialog.opened
                onTriggered: loadoptionsdialog.update()
            }

            onAccepted: {
                if (!root.maybe_loaded_loop) { 
                    console.log("Cannot load: loop not loaded")
                    return;
                }
                root.state_registry.load_action_started()
                try {
                    close()
                    var samplerate = root.maybe_loaded_loop.backend.get_sample_rate()
                    // Distribute file channels round-robin over loop channels.
                    // TODO: provide other options
                    var mapping = Array.from(Array(n_file_channels).keys()).map(v => [])
                    var fidx=0;
                    for(var cidx = 0; cidx < n_channels; cidx++) {
                        mapping[fidx].push(channels_to_load[cidx])
                        fidx = (fidx + 1) % n_file_channels
                    }
                    var task = file_io.load_soundfile_to_channels_async(filename, samplerate, null, mapping, 
                        update_audio_length_checkbox.checked ? root.maybe_loaded_loop : null)
                    task.when_finished( () => root.state_registry.load_action_finished() )
                } catch(e) {
                    root.state_registry.load_action_finished()
                    throw e
                }
            }

            Column {
                Grid {
                    columns: 2
                    verticalItemAlignment: Grid.AlignVCenter
                    columnSpacing: 10

                    Label {
                        text: "To direct channel(s):"
                        visible: loadoptionsdialog.direct_audio_channels.length > 0
                    }
                    CheckBox {
                        id: direct_load_checkbox
                        visible: loadoptionsdialog.direct_audio_channels.length > 0
                        checked: true
                        onCheckedChanged: loadoptionsdialog.update()
                    }

                    Label {
                        text: "To dry channel(s):"
                        visible: loadoptionsdialog.dry_audio_channels.length > 0
                    }
                    CheckBox {
                        id: dry_load_checkbox
                        visible: loadoptionsdialog.dry_audio_channels.length > 0
                        checked: true
                        onCheckedChanged: loadoptionsdialog.update()
                    }

                    Label {
                        text: "To wet channel(s):"
                        visible: loadoptionsdialog.wet_audio_channels.length > 0
                    }
                    CheckBox {
                        id: wet_load_checkbox
                        visible: loadoptionsdialog.wet_audio_channels.length > 0
                        checked: true
                        onCheckedChanged: loadoptionsdialog.update()
                    }

                    Label {
                        text: "Update loop length:"
                    }
                    CheckBox {
                        id: update_audio_length_checkbox
                        checked: true
                    }
                }

                Label {
                    visible: loadoptionsdialog.will_resample
                    wrapMode: Text.WordWrap
                    width: loadoptionsdialog.width - 50
                    text: "Warning: the file to load will be resampled. This may take a long time."
                }
            }
        }

        FileDialog {
            id: midiloaddialog
            fileMode: FileDialog.OpenFile
            acceptLabel: 'Load'
            nameFilters: ["Midi files (*.mid)"]
            onAccepted: {
                close()
                midiloadoptionsdialog.filename = selectedFile.toString().replace('file://', '');
                midiloadoptionsdialog.open()
            }
        }

        Dialog {
            id: midiloadoptionsdialog
            standardButtons: Dialog.Yes | Dialog.No
            Label { text: "Update loop length to loaded data length?" }
            property string filename
            property var channel : null
            function doLoad(update_loop_length) {
                root.force_load_backend()
                var samplerate = root.maybe_loaded_loop.backend.get_sample_rate()
                file_io.load_midi_to_channel_async(filename, samplerate, channel, update_loop_length ?
                    root.maybe_loaded_loop : null)
            }

            onAccepted: doLoad(true)
            onRejected: doLoad(false)
        }

        function popup () {
            menu.popup()
        }
    }
}
