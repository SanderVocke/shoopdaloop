import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import '../backend/frontend_interface/types.js' as Types

// The track control widget displays control buttons to control the
// (loops within a) track.
Item {
    id: trackctl
    width: childrenRect.width
    height: childrenRect.height

    property alias volume_dB: volume_slider.value
    property alias passthrough_dB: passthrough_slider.value
    property alias volume_dB_min: volume_slider.from
    property alias passthrough_dB_min: passthrough_slider.from

    readonly property bool in_is_stereo: audio_in_ports.length == 2
    readonly property bool out_is_stereo: audio_out_ports.length == 2

    property real initial_volume: {
        var volumes = initial_track_descriptor.ports
            .filter(p => is_out(p))
            .map(p => ('volume' in p) ? p.volume : undefined)
            .filter(p => p != undefined)
        return volumes.length > 0 ? Math.min(...volumes) : 1.0
    }
    property real initial_passthrough_volume: {
        var volumes = initial_track_descriptor.ports
            .filter(p => is_in(p))
            .map(p => ('volume' in p) ? p.volume : undefined)
            .filter(p => p != undefined)
        return volumes.length > 0 ? Math.min(...volumes) : 1.0
    }
    property real initial_volume_dB: 0.0
    onInitial_volumeChanged: {
        convert_volume.linear = initial_volume
        initial_volume_dB = convert_volume.dB
    }
    property real initial_passthrough_volume_dB: 0.0
    onInitial_passthrough_volumeChanged: {
        convert_passthrough.linear = initial_passthrough_volume
        initial_passthrough_volume_dB = convert_passthrough.dB
    }

    property bool muted: audio_out_ports.length > 0 ? audio_out_ports[0].muted : false
    property bool passthroughMuted: {
        var in_ports = [...audio_in_ports, ...midi_in_ports]
        return in_ports.length > 0 ? in_ports[0].passthrough_muted : false
    }

    property var initial_track_descriptor : null
    property Registry objects_registry : null
    property Registry state_registry : null

    property bool has_fx_chain : initial_track_descriptor.fx_chains.length == 1

    RegistryLookups {
        id: lookup_ports
        registry: trackctl.objects_registry
        keys: initial_track_descriptor ? initial_track_descriptor.ports.map((p) => p.id) : []
    }
    property alias ports : lookup_ports.objects

    function is_audio(p) { return p.schema.match(/audioport\.[0-9]+/) }
    function is_midi(p)  { return p.schema.match(/midiport\.[0-9]+/)  }
    function is_in(p)    { return p.direction == "input" && p.id.match(/.*_(?:in|direct)(?:_[0-9]*)?$/); }
    function is_out(p)   { return p.direction == "output" && p.id.match(/.*_(?:out|direct)(?:_[0-9]*)?$/); }

    property var audio_in_ports : {
        var r = ports.filter((p) => is_audio(p.descriptor) && is_in(p.descriptor))
        r.sort((a,b) => a.obj_id.localeCompare(b.obj_id))
        return r
    }
    property var audio_out_ports : {
        var r = ports.filter((p) => is_audio(p.descriptor) && is_out(p.descriptor))
        r.sort((a,b) => a.obj_id.localeCompare(b.obj_id))
        return r
    }
    property var midi_in_ports : ports.filter((p) => is_midi(p.descriptor) && is_in(p.descriptor))
    property var midi_out_ports : ports.filter((p) => is_midi(p.descriptor) && is_out(p.descriptor))

    property int n_midi_notes_active_in : 0
    property int n_midi_notes_active_out : 0
    property int n_midi_events_in : 0
    property int n_midi_events_out : 0
    function aggregate_midi_notes(ports) {
        var notes_per_port = ports.map((p) => p.n_notes_active)
        return Math.max(notes_per_port)
    }
    function aggregate_midi_events(ports) {
        var events_per_port = ports.map((p) => p.n_events_triggered)
        return Math.max(events_per_port)
    }
    function update_midi() {
        n_midi_notes_active_in = aggregate_midi_notes(midi_in_ports)
        n_midi_notes_active_out = aggregate_midi_notes(midi_out_ports)
        n_midi_events_in = aggregate_midi_events(midi_in_ports)
        n_midi_events_out = aggregate_midi_events(midi_out_ports)
    }
    onMidi_in_portsChanged: {
        midi_in_ports.forEach((m) => m.nNotesActiveChanged.connect(update_midi))
        midi_out_ports.forEach((m) => m.nNotesActiveChanged.connect(update_midi))
        midi_in_ports.forEach((m) => m.nEventsTriggeredChanged.connect(update_midi))
        midi_out_ports.forEach((m) => m.nEventsTriggeredChanged.connect(update_midi))
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

    LinearDbConversion {
        id: convert_volume
    }

    LinearDbConversion {
        id: convert_passthrough
    }

    function push_volume(volume, target) {
        convert_volume.dB = volume
        var v = convert_volume.linear
        if (target && target.volume != v) { target.set_volume(v) }
    }

    onVolume_dBChanged: {
        audio_out_ports.forEach((p) => push_volume(volume_dB, p))
    }

    onPassthrough_dBChanged: {
        audio_in_ports.forEach((p) => push_volume(passthrough_dB, p))
    }

    function toggle_muted() {
        var n = !muted
        audio_out_ports.forEach((p) => p.set_muted(n))
        midi_out_ports.forEach((p) => p.set_muted(n))
    }

    function toggle_passthroughMuted() {
        var n = !passthroughMuted
        audio_in_ports.forEach((p) => p.set_passthrough_muted(n))
        midi_in_ports.forEach((p) => p.set_passthrough_muted(n))
    }

    signal mute()
    signal unmute()

    Column {
        spacing: 2
        width: 100

        Item {
            height: childrenRect.height

            anchors {
                left: parent.left
                right: parent.right
            }
            ProgressBar {
                id: output_peak_bar_l
                visible: trackctl.out_is_stereo
                value: output_peak_meter_l.value
                padding: 2
                anchors {
                    left: parent.left
                    right: parent.horizontalCenter
                    verticalCenter: volume_row.verticalCenter
                }

                // Note: dB
                from: -50.0
                to: 0.0

                AudioLevelMeterModel {
                    id: output_peak_meter_l
                    max_dt: 0.1

                    input: trackctl.audio_out_ports.length > 0 ? trackctl.audio_out_ports[0].peak : 0.0
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
                        color: '#555555'
                        x: (1.0 - output_peak_bar_l.visualPosition) * parent.width
                    }
                }
            }
            ProgressBar {
                id: output_peak_bar_r
                visible: trackctl.out_is_stereo
                value: output_peak_meter_r.value
                padding: 2
                anchors {
                    left: parent.horizontalCenter
                    right: parent.right
                    verticalCenter: volume_row.verticalCenter
                }

                // Note: dB
                from: -50.0
                to: 0.0

                AudioLevelMeterModel {
                    id: output_peak_meter_r
                    max_dt: 0.1

                    input: trackctl.audio_out_ports.length > 1 ? trackctl.audio_out_ports[1].peak : 0.0
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
                        color: '#555555'
                    }
                }
            }
            ProgressBar {
                id: output_peak_bar_overall
                visible: !trackctl.out_is_stereo
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

                    input: trackctl.audio_out_ports.length > 0 ? Math.max(...trackctl.audio_out_ports.map(p => p.peak)) : 0.0
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
                        color: '#555555'
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
                visible: trackctl.n_midi_notes_active_out > 0
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
                visible: trackctl.n_midi_events_out > 0
            }
            Row {
                spacing: -4
                id: volume_row

                Item {
                    width: 18
                    height: width
                    anchors.verticalCenter: volume_slider.verticalCenter

                    Rectangle {
                        anchors.fill: parent
                        visible: volume_mouse_area.containsMouse
                        color: '#555555'
                        radius: width/2
                    }
                    MouseArea {
                        id: volume_mouse_area
                        anchors.fill: parent
                        acceptedButtons: Qt.LeftButton
                        hoverEnabled: true

                        onClicked: trackctl.toggle_muted()

                        MaterialDesignIcon {
                            size: parent.width
                            name: trackctl.muted ? 'volume-mute' : 'volume-high'
                            color: trackctl.muted ? 'grey' : Material.foreground
                            anchors.fill: parent
                        }
                    }
                }
                AudioSlider {
                    id: volume_slider
                    orientation: Qt.Horizontal
                    width: trackctl.out_is_stereo ? 70 : 85
                    height: 20
                    from: -30.0
                    to: 20.0
                    value: 0.0
                    property var initial_accent

                    handle.width: 4

                    Material.accent: trackctl.muted ? 'grey' : trackctl.Material.accent

                    property real initial_value_dB: trackctl.initial_volume_dB
                    onInitial_value_dBChanged: value = initial_value_dB
                    Component.onCompleted: value = initial_value_dB

                    tooltip: 'Track volume. Double-click to reset.'
                    value_tooltip_postfix: " dB"
                    show_value_tooltip: true
                }

                AudioDial {
                    id: output_balance_dial
                    visible: trackctl.out_is_stereo
                    from: -1.0
                    to:   1.0
                    value: 0.0

                    width: 18
                    height: 18

                    anchors.verticalCenter: volume_slider.verticalCenter

                    tooltip: 'Track stereo balance. Double-click to reset.'
                    label: 'B'

                    onMoved: {
                        // TODO
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
                visible: trackctl.in_is_stereo
                value: input_peak_meter_l.value
                padding: 2
                anchors {
                    left: parent.left
                    right: parent.horizontalCenter
                    verticalCenter: passthrough_row.verticalCenter
                }

                // Note: dB
                from: -50.0
                to: 0.0

                AudioLevelMeterModel {
                    id: input_peak_meter_l
                    max_dt: 0.1

                    input: trackctl.audio_in_ports.length > 0 ? trackctl.audio_in_ports[0].peak * trackctl.audio_in_ports[0].passthrough_volume : 0.0
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
                        color: '#555555'
                        x: parent.width - input_peak_l_bar.visualPosition * parent.width
                    }
                }
            }
            ProgressBar {
                id: input_peak_r_bar
                visible: trackctl.in_is_stereo
                value: input_peak_meter_r.value
                padding: 2
                anchors {
                    left: parent.horizontalCenter
                    right: parent.right
                    verticalCenter: passthrough_row.verticalCenter
                }

                // Note: dB
                from: -50.0
                to: 0.0

                AudioLevelMeterModel {
                    id: input_peak_meter_r
                    max_dt: 0.1

                    input: trackctl.audio_in_ports.length > 1 ? trackctl.audio_in_ports[1].peak * trackctl.audio_in_ports[1].passthrough_volume : 0.0
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
                        color: '#555555'
                    }
                }
            }
            ProgressBar {
                id: input_peak_overall_bar
                visible: !trackctl.in_is_stereo
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

                    input: trackctl.audio_in_ports.length > 0 ? Math.max(...trackctl.audio_in_ports.map(p => p.peak)) * Math.max(...trackctl.audio_in_ports.map(p => p.passthrough_volume)) : 0.0
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
                        color: '#555555'
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
                color: '#00BBFF'
                visible: trackctl.n_midi_notes_active_in > 0
            }
            Rectangle {
                id: input_midi_evts_indicator
                anchors {
                    right: input_peak_r_bar.right
                    top: input_peak_r_bar.top
                    bottom: input_peak_r_bar.bottom
                }
                width: 8
                radius: 2
                color: '#00FFFF'
                visible: trackctl.n_midi_events_in > 0
            }
            Row {
                id: passthrough_row
                spacing: -4

                Item {
                    width: 18
                    height: width
                    anchors.verticalCenter: passthrough_slider.verticalCenter

                    Rectangle {
                        anchors.fill: parent
                        visible: passthrough_mouse_area.containsMouse
                        color: '#555555'
                        radius: width/2
                    }
                    MouseArea {
                        id: passthrough_mouse_area
                        anchors.fill: parent
                        acceptedButtons: Qt.LeftButton
                        hoverEnabled: true

                        onClicked: trackctl.toggle_passthroughMuted()

                        MaterialDesignIcon {
                            size: parent.width
                            name: 'ear-hearing'
                            color: trackctl.passthroughMuted ? 'grey' : Material.foreground
                            anchors.fill: parent
                        }
                    }
                }
                AudioSlider {
                    id: passthrough_slider
                    orientation: Qt.Horizontal
                    width: trackctl.in_is_stereo ? 70 : 85
                    height: 20
                    from: -30.0
                    to: 20.0
                    value: 0.0
                    property var initial_accent
                    
                    handle.width: 4

                    Material.accent: trackctl.passthroughMuted ? 'grey' : trackctl.Material.accent

                    property real initial_value_dB: trackctl.initial_passthrough_volume_dB
                    onInitial_value_dBChanged: value = initial_value_dB
                    Component.onCompleted: value = initial_value_dB

                    tooltip: 'Track input volume. Double-click to reset.'
                    show_value_tooltip: true
                    value_tooltip_postfix: " dB"
                }

                AudioDial {
                    id: passthrough_balance_dial
                    visible: trackctl.in_is_stereo
                    from: -1.0
                    to:   1.0
                    value: 0.0

                    width: 18
                    height: 18

                    anchors.verticalCenter: passthrough_slider.verticalCenter

                    onMoved: {
                        // TODO
                    }

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

        // TODO: reinstate panning
        //Row {
        //    spacing: -2

        //    MaterialDesignIcon {
        //        size: 15
        //        name: 'unfold-more-vertical'
        //        color: Material.foreground
        //        anchors.verticalCenter: pan.verticalCenter
        //    }
        //    Slider {
        //        id: pan
        //        orientation: Qt.Horizontal
        //        width: 90
        //        height: 20
        //        from: -1.0
        //        to: 1.0
        //        value: 0.0
        //        ToolTip {
        //            delay: 1000
        //            visible: pan_ma.containsMouse
        //            text: 'Pan control. Right-click to center.'
        //        }
        //        MouseArea {
        //            id: pan_ma
        //            hoverEnabled: true
        //            anchors.fill: parent
        //            acceptedButtons: Qt.RightButton
        //            onClicked: (ev) => { pan.value = 0.0; }
        //        }
        //    }
        //}
    }
}
