import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6

import ShoopDaLoop.PythonLogger
import ShoopConstants

// The track control widget displays control buttons to control the
// (loops within a) track.
Item {
    id: root
    height: childrenRect.height

    // Input properties
    property var initial_track_descriptor : null

    // UI-controlled properties
    property alias gain_dB: gain_fader.value
    property alias input_gain_dB: input_fader.value
    property alias gain_dB_min: gain_fader.from
    property alias input_gain_dB_min: input_fader.from
    property alias input_balance: input_balance_dial.value
    property alias output_balance: output_balance_dial.value
    property real gain_fader_position: gain_fader.position
    property real input_fader_position: input_fader.position

    property bool monitor : {
        // Initial setting
        var ports = initial_track_descriptor.ports
            .filter(p => is_in(p));
        var settings = new Set();
        ports.forEach((p) => { settings.add(!p.passthrough_muted) })
        if (settings.size == 1) {
            root.logger.debug("Initial monitor setting: " + Array.from(settings)[0])
            return Array.from(settings)[0]
        }
        root.logger.debug("Initial monitor setting not found")
        return false
    }
        
    property bool mute : {
        // Initial setting
        var ports = initial_track_descriptor.ports
            .filter(p => is_out(p));
        var settings = new Set();
        ports.forEach((p) => { settings.add(p.muted) })
        if (settings.size == 1) {
            root.logger.debug("Initial mute setting: " + Array.from(settings)[0])
            return Array.from(settings)[0]
        }
        root.logger.debug("Initial mute setting not found")
        return false
    }

    // Readonlies
    readonly property PythonLogger logger : PythonLogger { name: "Frontend.Qml.TrackControlWidget" }
    readonly property bool in_is_stereo: audio_in_ports.length == 2
    readonly property bool out_is_stereo: audio_out_ports.length == 2
    readonly property var initial_output_gain_and_balance: {
        var ports = initial_track_descriptor.ports
            .filter(p => is_out(p));
        ports.sort((a,b) => a.id.localeCompare(b.id))
        return gain_and_balance_from_gains(
            ports.map(p => ('gain' in p) ? p.gain : undefined)
            .filter(p => p != undefined)
        );
    }
    readonly property var initial_input_gain_and_balance: {
        var ports = initial_track_descriptor.ports
            .filter(p => is_in(p));
        ports.sort((a,b) => a.id.localeCompare(b.id))
        return gain_and_balance_from_gains(
            ports.map(p => ('gain' in p) ? p.gain : undefined)
            .filter(p => p != undefined)
        );
    }
    property real initial_gain: initial_output_gain_and_balance["gain"]
    property real initial_balance: initial_output_gain_and_balance["balance"]
    property real initial_input_gain: initial_input_gain_and_balance["gain"]
    property real initial_input_balance: initial_input_gain_and_balance["balance"]
    readonly property bool has_fx_chain : initial_track_descriptor && 'fx_chain' in initial_track_descriptor
    readonly property alias ports : lookup_ports.objects
    readonly property alias loops : lookup_loops.objects
    readonly property alias maybe_fx_chain : lookup_fx_chain.object
    readonly property alias fx_ports : lookup_fx_ports.objects
    readonly property var audio_in_ports : {
        var r = ports.filter((p) => p && is_audio(p.descriptor) && is_in(p.descriptor))
        r.sort((a,b) => a.obj_id.localeCompare(b.obj_id))
        return r
    }
    readonly property var audio_out_ports : {
        var r = ports.filter((p) => p && is_audio(p.descriptor) && is_out(p.descriptor))
        r.sort((a,b) => a.obj_id.localeCompare(b.obj_id))
        return r
    }
    readonly property var fx_out_ports : {
        var r = fx_ports.filter((p) => p && is_audio(p.descriptor) && p.descriptor.id.match(/_out_/))
        r.sort((a,b) => a.obj_id.localeCompare(b.obj_id))
        return r
    }
    readonly property var midi_in_ports : ports.filter((p) => p && is_midi(p.descriptor) && is_in(p.descriptor))
    readonly property var midi_out_ports : ports.filter((p) => p && is_midi(p.descriptor) && is_out(p.descriptor))
    readonly property var out_balance_gain_factor_l: balance_gain_factor_l(output_balance)
    readonly property var in_balance_gain_factor_l: balance_gain_factor_l(input_balance)
    readonly property var out_balance_gain_factor_r: balance_gain_factor_r(output_balance)
    readonly property var in_balance_gain_factor_r: balance_gain_factor_r(input_balance)
    readonly property var unique_loop_modes : {
        let modes = root.loops.map(l => l.mode)
        return Array.from(new Set(modes))
    }
        readonly property var unique_next_cycle_loop_modes : {
        let modes = root.loops.map(l => {
            if (l.next_transition_delay == 1) { return l.next_mode; }
            else { return undefined }
        }).filter(m => "undefined" !== typeof m)
        return Array.from(new Set(modes))
    }
    readonly property var control_logic : logic

    // Controlled properties
    property real initial_gain_dB: 0.0
    property real initial_input_gain_dB: 0.0
    property int n_midi_notes_active_in : 0
    property int n_midi_notes_active_out : 0
    property int n_midi_events_in : 0
    property int n_midi_events_out : 0

    // Handlers
    onInitial_gainChanged: {
        // Will lead to updating the faders
        convert_gain.linear = initial_gain
        initial_gain_dB = convert_gain.dB
    }
    onInitial_input_gainChanged: {
        // Will lead to updating the faders
        convert_gain.linear = initial_input_gain
        initial_input_gain_dB = convert_gain.dB
    }
    onMidi_in_portsChanged: {
        midi_in_ports.forEach((m) => m.nInputNotesActiveChanged.connect(update_midi))
        midi_out_ports.forEach((m) => m.nOutputNotesActiveChanged.connect(update_midi))
        midi_in_ports.forEach((m) => m.nInputEventsChanged.connect(update_midi))
        midi_out_ports.forEach((m) => m.nOutputEventsChanged.connect(update_midi))
    }
    onGain_dBChanged: push_out_gains()
    onInput_gain_dBChanged: push_in_gains()
    onOutput_balanceChanged: push_out_gains()
    onInput_balanceChanged: push_in_gains()
    Connections {
        target: logic
        function onForce_monitoring_offChanged() {
            if (logic.force_monitoring_off) {
                monitor = false
            }
        }
        function onMute_drywet_input_passthroughChanged() { root.push_monitor() }
        function onMute_drywet_output_passthroughChanged() { root.push_monitor() }
        function onMute_direct_passthroughChanged() { root.push_monitor() }
        function onEnable_fxChanged() { root.push_fx_active() }
    }
    onPortsChanged: logic.trigger_signals()
    onMaybe_fx_chainChanged: logic.trigger_signals()
    onFx_out_portsChanged: logic.trigger_signals()
    onMuteChanged: push_mute()

    property string object_schema : ''

    // Registration
    RegisterInRegistry {
        id: reg_entry
        registry: registries.objects_registry
        object: root
        key: root.initial_track_descriptor.id + "_control_widget"
    }

    // Helpers
    TrackControlLogic {
        id: logic
        monitor: root.monitor

        unique_loop_modes : root.unique_loop_modes
        unique_next_cycle_loop_modes : root.unique_next_cycle_loop_modes
    }
    RegistryLookups {
        id: lookup_loops
        registry: registries.objects_registry
        keys: root.initial_track_descriptor ? root.initial_track_descriptor.loops.map((l) => l.id) : []
    }
    RegistryLookups {
        id: lookup_ports
        registry: registries.objects_registry
        keys: initial_track_descriptor ? initial_track_descriptor.ports.map((p) => p.id) : []
    }
    RegistryLookups {
        id: lookup_fx_ports
        registry: registries.objects_registry
        keys: (initial_track_descriptor && initial_track_descriptor.fx_chain) ? initial_track_descriptor.fx_chain.ports.map((p) => p.id) : []
    }
    RegistryLookup {
        id: lookup_fx_chain
        registry: registries.objects_registry
        key: (initial_track_descriptor && initial_track_descriptor.fx_chain) ? initial_track_descriptor.fx_chain.id : null
    }
    LinearDbConversion {
        id: convert_gain
    }
    LinearDbConversion {
        id: convert_input_gain
    }

    // Methods
    function is_audio(p)  { return p && p.schema.match(/audioport\.[0-9]+/) }
    function is_midi(p)   { return p && p.schema.match(/midiport\.[0-9]+/)  }
    function is_in(p)     { return p && p.id.match(/.*_in(?:_[0-9]*)?$/); }
    function is_out(p)    { return p && p.id.match(/.*_out(?:_[0-9]*)?$/); }
    function is_dry(p)    { return p && p.id.match(/.*_dry_.*/); }
    function is_wet(p)    { return p && p.id.match(/.*_wet_.*/); }
    function is_direct(p) { return p && p.id.match(/.*_direct_.*/); }
    function aggregate_midi_notes(ports) {
        var notes_per_port = ports.map((p) => p.n_input_notes_active)
        return Math.max(notes_per_port)
    }
    function aggregate_midi_events(ports) {
        var events_per_port = ports.map((p) => p.n_input_events)
        return Math.max(events_per_port)
    }
    function update_midi() {
        n_midi_notes_active_in = aggregate_midi_notes(midi_in_ports)
        n_midi_notes_active_out = aggregate_midi_notes(midi_out_ports)
        n_midi_events_in = aggregate_midi_events(midi_in_ports)
        n_midi_events_out = aggregate_midi_events(midi_out_ports)
    }
    function find_nth(array, n, fn) {
        var _n=0
        for(var i=0; i<array.length; i++) {
            var match = fn(array[i])
            if (match && _n >= n) { return array[i] }
            if (match) { _n++ }
        }
        return null;
    }
    function set_gain(gain) {
        gain_dB = Math.min(Math.max(gain_fader.gainToDb(gain), gain_fader.from), gain_fader.to)
        push_out_gains()
    }
    function set_balance(balance) {
        output_balance = balance
        push_out_gains()
    }
    function set_gain_fader(value) {
        gain_fader.value = gain_fader.valueAt(value)
        push_out_gains()
    }
    function set_input_gain(gain) {
        input_gain_dB = Math.min(Math.max(input_fader.gainToDb(gain), input_fader.from), input_fader.to)
        push_in_gains()
    }
    function set_input_gain_fader(value) {
        input_fader.value = input_fader.valueAt(value)
        push_in_gains()
    }
    function convert_gain_to_linear(gain) {
        convert_gain.dB = gain
        return convert_gain.linear
    }
    function push_gain(gain, target, gain_factor = 1.0) {
        convert_gain.dB = gain
        var v = convert_gain.linear * gain_factor
        logger.trace(() => ("Pushing gain " + v + " to " + target.obj_id))
        if (target && target.gain != v) { target.set_gain(v) }
    }
    function toggle_muted() { mute = !mute }
    function toggle_monitor() { monitor = !monitor }
    function balance_gain_factor_l(balance) {
        return (balance > 0.0) ? 1.0 - balance : 1.0
    }
    function balance_gain_factor_r(balance) {
        return (balance < 0.0) ? 1.0 + balance : 1.0
    }
    function gain_and_balance_from_gains(gains) {
        let overall_gain = gains.length > 0 ? Math.min(...gains) : 1.0
        if (gains.length == 2) {
            // Stereo handling
            let vol = Math.max(...gains)
            let weak_factor = Math.min(...gains) / vol
            let weak_is_left = gains[0] < gains[1]
            let balance = weak_is_left ? (1.0 - weak_factor) : -(1.0 - weak_factor)
            return {
                "gain": vol,
                "balance": balance
            }
        }
        else {
            return {
                "gain": overall_gain,
                "balance": 0.0
            }
        }
    }
    property var last_pushed_in_gain: null
    property var last_pushed_gain: null
    property var last_pushed_in_stereo_balance: null
    property var last_pushed_out_stereo_balance: null
    function push_in_gains() {
        audio_in_ports.forEach((p, idx) => {
            if (idx == 0 && in_is_stereo) { push_gain(input_gain_dB, p, in_balance_gain_factor_l) }
            else if (idx == 1 && in_is_stereo) { push_gain(input_gain_dB, p, in_balance_gain_factor_r) }
            else { push_gain(input_gain_dB, p) }
        })
        last_pushed_in_gain = convert_gain_to_linear(input_gain_dB)
        if (in_is_stereo) {
            last_pushed_in_stereo_balance = input_balance
        }
    }
    function push_out_gains() {
        if (gain_dB === undefined) {
            throw new Error("Cannot push undefined gain")
        }
        audio_out_ports.forEach((p, idx) => {
            if (idx == 0 && out_is_stereo) { push_gain(gain_dB, p, out_balance_gain_factor_l) }
            else if (idx == 1 && out_is_stereo) { push_gain(gain_dB, p, out_balance_gain_factor_r) }
            else { push_gain(gain_dB, p) }
        })
        last_pushed_gain = convert_gain_to_linear(gain_dB)
        if (out_is_stereo) {
            last_pushed_out_stereo_balance = output_balance
        }
    }
    function push_mute() {
        if (mute === undefined) {
            throw new Error("Cannot push undefined mute")
        }
        audio_out_ports.forEach((p) => {if (p) { p.set_muted(mute) } else { logger.warn("Undefined audio out port") }})
        midi_out_ports.forEach((p) => {if (p) { p.set_muted(mute) } else { logger.warn("Undefined audio out port") }})
    }
    function push_monitor() {
        if (monitor === undefined) {
            throw new Error("Cannot push undefined monitor")
        }
        audio_in_ports
            .filter(p => is_dry(p.descriptor))
            .forEach((p) => {
                p.set_passthrough_muted(logic.mute_drywet_input_passthrough)
            })
        audio_in_ports
            .filter(p => is_direct(p.descriptor))
            .forEach((p) => {
                p.set_passthrough_muted(logic.mute_direct_passthrough)
            })
        midi_in_ports
            .filter(p => is_direct(p.descriptor))
            .forEach((p) => {
                p.set_passthrough_muted(logic.mute_direct_passthrough)
            })
        midi_in_ports
            .filter(p => is_dry(p.descriptor))
            .forEach((p) => {
                p.set_passthrough_muted(logic.mute_drywet_input_passthrough)
            })
        fx_out_ports
            .forEach((p) => {
                p.set_passthrough_muted(logic.mute_drywet_output_passthrough)
            })
    }
    function push_fx_active() {
        if (root.maybe_fx_chain) {
            root.maybe_fx_chain.set_active(logic.enable_fx)
        }
    }
    Component.onCompleted: {
        push_monitor()
        push_mute()
        push_out_gains()
        push_in_gains()
        push_fx_active()
    }

    signal mute()
    signal unmute()

    // UI

    Column {
        spacing: 2
        
        anchors {
            left: parent.left
            right: parent.right
            leftMargin: 2
            rightMargin: 2
        }

        Item {
            height: childrenRect.height

            anchors {
                left: parent.left
                right: parent.right
            }
            ProgressBar {
                id: output_peak_bar_l
                visible: root.out_is_stereo
                value: output_peak_meter_l.value
                padding: 2
                anchors {
                    left: parent.left
                    right: parent.horizontalCenter
                    verticalCenter: gain_row.verticalCenter
                }

                // Note: dB
                from: -50.0
                to: 0.0

                AudioLevelMeterModel {
                    id: output_peak_meter_l
                    max_dt: 0.1

                    input: root.audio_out_ports.length > 0 ? root.audio_out_ports[0].input_peak : 0.0
                }

                background: Rectangle {
                    implicitWidth: 25
                    implicitHeight: 16
                    color: Material.background
                    radius: 3
                }

                contentItem: Item {
                    implicitWidth: 25
                    implicitHeight: 16

                    Rectangle {
                        width: output_peak_bar_l.visualPosition * parent.width
                        height: parent.height
                        radius: 2
                        color: '#666666'
                        x: (1.0 - output_peak_bar_l.visualPosition) * parent.width
                    }
                }
            }
            ProgressBar {
                id: output_peak_bar_r
                visible: root.out_is_stereo
                value: output_peak_meter_r.value
                padding: 2
                anchors {
                    left: parent.horizontalCenter
                    right: parent.right
                    verticalCenter: gain_row.verticalCenter
                }

                // Note: dB
                from: -50.0
                to: 0.0

                AudioLevelMeterModel {
                    id: output_peak_meter_r
                    max_dt: 0.1

                    input: root.audio_out_ports.length > 1 ? root.audio_out_ports[1].input_peak : 0.0
                }

                background: Rectangle {
                    implicitWidth: 25
                    implicitHeight: 16
                    color: Material.background
                    radius: 3
                }

                contentItem: Item {
                    implicitWidth: 25
                    implicitHeight: 16

                    Rectangle {
                        width: output_peak_bar_r.visualPosition * parent.width
                        height: parent.height
                        radius: 2
                        color: '#666666'
                    }
                }
            }
            ProgressBar {
                id: output_peak_bar_overall
                visible: !root.out_is_stereo
                value: output_peak_meter_overall.value
                anchors {
                    left: output_peak_bar_l.left
                    right: output_peak_bar_r.right
                    top: output_peak_bar_l.top
                    bottom: output_peak_bar_l.bottom
                }

                // Note: dB
                from: -50.0
                to: 0.0

                AudioLevelMeterModel {
                    id: output_peak_meter_overall
                    max_dt: 0.1

                    input: root.audio_out_ports.length > 0 ? Math.max(...root.audio_out_ports.map(p => p.input_peak)) : 0.0
                }

                background: Rectangle {
                    implicitWidth: 25
                    implicitHeight: 16
                    color: Material.background
                    radius: 3
                }

                contentItem: Item {
                    implicitWidth: 25
                    implicitHeight: 16

                    Rectangle {
                        width: output_peak_bar_overall.visualPosition * parent.width
                        height: parent.height
                        radius: 2
                        color: '#666666'
                    }
                }
            }
            Rectangle {
                id: output_midi_indicator
                anchors {
                    right: output_peak_bar_r.right
                    top: output_peak_bar_r.top
                    bottom: output_peak_bar_r.bottom
                }
                width: 8
                radius: 2
                color: '#00BBFF'
                visible: root.n_midi_notes_active_out > 0
            }
            Rectangle {
                id: output_midi_evts_indicator
                anchors {
                    right: output_peak_bar_r.right
                    top: output_peak_bar_r.top
                    bottom: output_peak_bar_r.bottom
                }
                width: 8
                radius: 2
                color: '#00FFFF'
                visible: root.n_midi_events_out > 0
            }

            Item {
                id: gain_row
                height: childrenRect.height

                anchors {
                    left: parent.left
                    right: parent.right
                }

                Item {
                    width: 18
                    height: 18
                    id: gain_row_button

                    anchors {
                        verticalCenter: gain_fader.verticalCenter
                        left: gain_row.left
                    }

                    visible: (root.audio_out_ports.length + root.midi_out_ports.length) > 0

                    Rectangle {
                        anchors.fill: parent
                        visible: gain_mouse_area.containsMouse
                        color: '#555555'
                        radius: width/2
                    }
                    MouseArea {
                        id: gain_mouse_area
                        anchors.fill: parent
                        acceptedButtons: Qt.LeftButton
                        hoverEnabled: true

                        onClicked: root.toggle_muted()

                        MaterialDesignIcon {
                            size: parent.width
                            name: root.mute ? 'volume-mute' : 'volume-high'
                            color: root.mute ? 'grey' : Material.foreground
                            anchors.fill: parent
                        }
                        ToolTip {
                            delay: 1000
                            visible: vol_ma.containsMouse
                            text: 'Mute/unmute output.'
                        }
                        MouseArea {
                            id: vol_ma
                            hoverEnabled: true
                            anchors.fill: parent
                            acceptedButtons: Qt.NoButton
                        }
                    }
                }
                AudioSlider {
                    id: gain_fader

                    anchors {
                        left: gain_row_button.right
                        right: output_balance_dial.left
                        leftMargin: -4
                        rightMargin: -4
                        verticalCenter: gain_row_button.verticalCenter
                    }

                    visible: root.audio_out_ports.length > 0
                    orientation: Qt.Horizontal                

                    height: 20
                    from: -30.0
                    to: 20.0
                    value: 0.0
                    property var initial_accent

                    handle.width: 4

                    Material.accent: root.mute ? 'grey' : root.Material.accent

                    property alias initial_value_dB: root.initial_gain_dB
                    onInitial_value_dBChanged: value = initial_value_dB
                    Component.onCompleted: value = initial_value_dB

                    tooltip: 'Track gain. Double-click to reset.'
                    value_tooltip_postfix: " dB"
                    show_value_tooltip: true
                }

                AudioDial {
                    id: output_balance_dial
                    visible: root.out_is_stereo
                    from: -1.0
                    to:   1.0
                    value: 0.0

                    width: root.out_is_stereo ? 18 : 0
                    height: 18

                    anchors {
                        verticalCenter: gain_fader.verticalCenter
                        right: gain_row.right
                    }

                    property alias initial_value: root.initial_balance
                    onInitial_valueChanged: value = initial_value
                    Component.onCompleted: value = initial_value

                    tooltip: 'Track stereo balance. Double-click to reset.'
                    label: 'B'

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
                }
            }
        }

        Item {
            height: childrenRect.height

            anchors {
                left: parent.left
                right: parent.right
            }
            ProgressBar {
                id: input_peak_l_bar
                visible: root.in_is_stereo
                value: input_peak_meter_l.value
                padding: 2
                anchors {
                    left: parent.left
                    right: parent.horizontalCenter
                    verticalCenter: monitor_row.verticalCenter
                }

                // Note: dB
                from: -50.0
                to: 0.0

                AudioLevelMeterModel {
                    id: input_peak_meter_l
                    max_dt: 0.1

                    input: root.audio_in_ports.length > 0 ? root.audio_in_ports[0].input_peak : 0.0
                }

                background: Rectangle {
                    implicitWidth: 25
                    implicitHeight: 16
                    color: Material.background
                    radius: 3
                }

                contentItem: Item {
                    implicitWidth: 25
                    implicitHeight: 16

                    Rectangle {
                        width: input_peak_l_bar.visualPosition * parent.width
                        height: parent.height
                        radius: 2
                        color: '#666666'
                        x: parent.width - input_peak_l_bar.visualPosition * parent.width
                    }
                }
            }
            ProgressBar {
                id: input_peak_r_bar
                visible: root.in_is_stereo
                value: input_peak_meter_r.value
                padding: 2
                anchors {
                    left: parent.horizontalCenter
                    right: parent.right
                    verticalCenter: monitor_row.verticalCenter
                }

                // Note: dB
                from: -50.0
                to: 0.0

                AudioLevelMeterModel {
                    id: input_peak_meter_r
                    max_dt: 0.1

                    input: root.audio_in_ports.length > 1 ? root.audio_in_ports[1].input_peak : 0.0
                }

                background: Rectangle {
                    implicitWidth: 25
                    implicitHeight: 16
                    color: Material.background
                    radius: 3
                }

                contentItem: Item {
                    implicitWidth: 25
                    implicitHeight: 16

                    Rectangle {
                        width: input_peak_r_bar.visualPosition * parent.width
                        height: parent.height
                        radius: 2
                        color: '#666666'
                    }
                }
            }
            ProgressBar {
                id: input_peak_overall_bar
                visible: !root.in_is_stereo
                value: input_peak_meter_overall.value
                anchors {
                    left: input_peak_l_bar.left
                    right: input_peak_r_bar.right
                    bottom: input_peak_l_bar.bottom
                    top: input_peak_l_bar.top
                }

                // Note: dB
                from: -50.0
                to: 0.0

                AudioLevelMeterModel {
                    id: input_peak_meter_overall
                    max_dt: 0.1

                    input: root.audio_in_ports.length > 0 ? Math.max(...root.audio_in_ports.map(p => p.input_peak)) : 0.0
                }

                background: Rectangle {
                    implicitWidth: 25
                    implicitHeight: 16
                    color: Material.background
                    radius: 3
                }

                contentItem: Item {
                    implicitWidth: 25
                    implicitHeight: 16

                    Rectangle {
                        width: input_peak_overall_bar.visualPosition * parent.width
                        height: parent.height
                        radius: 2
                        color: '#666666'
                    }
                }
            }
            Rectangle {
                id: input_midi_indicator
                anchors {
                    right: input_peak_r_bar.right
                    top: input_peak_r_bar.top
                    bottom: input_peak_r_bar.bottom
                }
                width: 8
                radius: 2
                color: '#00FFFF'
                visible: root.n_midi_notes_active_in > 0 || root.n_midi_events_in > 0
            }
            Item {
                id: monitor_row
                
                height: childrenRect.height

                anchors {
                    left: parent.left
                    right: parent.right
                }

                Item {
                    width: 18
                    height: 18
                    id: monitor_row_button
                    
                    anchors {
                        verticalCenter: input_fader.verticalCenter
                        left: monitor_row.left
                    }

                    visible: (root.audio_in_ports.length + root.midi_in_ports.length) > 0

                    Rectangle {
                        anchors.fill: parent
                        visible: monitor_mouse_area.containsMouse
                        color: '#555555'
                        radius: width/2
                    }
                    MouseArea {
                        id: monitor_mouse_area
                        anchors.fill: parent
                        acceptedButtons: Qt.LeftButton
                        hoverEnabled: true

                        onClicked: root.toggle_monitor()

                        MaterialDesignIcon {
                            size: parent.width
                            name: 'ear-hearing'
                            color: root.monitor ? Material.foreground : 'grey'
                            anchors.fill: parent
                        }
                        ToolTip {
                            delay: 1000
                            visible: pt_ma.containsMouse
                            text: 'Enable/disable onitoring.'
                        }
                        MouseArea {
                            id: pt_ma
                            hoverEnabled: true
                            anchors.fill: parent
                            acceptedButtons: Qt.NoButton
                        }
                    }
                }
                AudioSlider {
                    id: input_fader

                    anchors {
                        left: monitor_row_button.right
                        right: input_balance_dial.left
                        leftMargin: -4
                        rightMargin: -4
                        verticalCenter: monitor_row_button.verticalCenter
                    }

                    visible: root.audio_in_ports.length > 0

                    orientation: Qt.Horizontal
                    height: 20
                    from: -30.0
                    to: 20.0
                    value: 0.0
                    property var initial_accent
                    
                    handle.width: 4

                    Material.accent: root.monitor ? root.Material.accent : 'grey'

                    property real initial_value_dB: root.initial_input_gain_dB
                    onInitial_value_dBChanged: value = initial_value_dB
                    Component.onCompleted: value = initial_value_dB

                    tooltip: 'Track input gain. Double-click to reset.'
                    show_value_tooltip: true
                    value_tooltip_postfix: " dB"
                }

                AudioDial {
                    id: input_balance_dial
                    visible: root.in_is_stereo
                    from: -1.0
                    to:   1.0
                    value: 0.0

                    width: root.in_is_stereo ? 18 : 0
                    height: 18

                    anchors {
                        verticalCenter: input_fader.verticalCenter
                        right: monitor_row.right
                    }

                    property real initial_value: root.initial_input_balance
                    onInitial_valueChanged: value = initial_value
                    Component.onCompleted: value = initial_value

                    handle.width: 4
                    handle.height: 4

                    show_value_tooltip: true
                    
                    background: Rectangle {
                        radius: width / 2.0
                        width: parent.width
                        color: '#222222'
                        border.width: 1
                        border.color: 'grey'
                    }

                    tooltip: 'Track input stereo balance. Double-click to reset.'
                    label: 'B'
                }
            }
        }
    }
}
