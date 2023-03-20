import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import '../../build/types.js' as Types

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

    property bool muted: audio_out_ports.length > 0 ? audio_out_ports[0].muted : false
    property bool passthroughMuted: audio_in_ports.length > 0 ? audio_in_ports[0].muted : false

    property var initial_track_descriptor : null
    property Registry objects_registry : null
    property Registry state_registry : null

    RegistryLookups {
        id: lookup_ports
        registry: trackctl.objects_registry
        keys: initial_track_descriptor ? initial_track_descriptor.ports.map((p) => p.id) : []
    }

    property alias ports : lookup_ports.objects
    property var audio_in_ports : ports.filter((p) => p instanceof AudioPort && p.direction == Types.PortDirection.Input)
    property var audio_out_ports : ports.filter((p) => p instanceof AudioPort && p.direction == Types.PortDirection.Output)
    property var midi_in_ports : ports.filter((p) => p instanceof MidiPort && p.direction == Types.PortDirection.Input)
    property var midi_out_ports : ports.filter((p) => p instanceof MidiPort && p.direction == Types.PortDirection.Output)

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

    function push_volume(target) {
        convert_volume.dB = volume_dB
        var v = convert_volume.linear
        if (target && target.volume != v) { target.set_backend_volume(v) }
    }

    onVolume_dBChanged: {
        trackctl.audio_out_ports.forEach((p) => push_volume(p))
    }

    onPassthrough_dBChanged: {
        audio_in_ports.forEach((p) => push_volume(p))
    }

    // Connections {
    //     target: ports_manager || null
    //     function onVolumeChanged() {
    //         convert_volume.linear = ports_manager.volume
    //         volume_dB = convert_volume.dB
    //     }
    //     function onPassthroughChanged() {
    //         convert_passthrough.linear = ports_manager.passthrough
    //         passthrough_dB = convert_passthrough.dB
    //     }
    // }

    // Connections {
    //     target: ports_manager
    //     function onMutedChanged() { trackctl.muted = ports_manager.muted }
    //     function onPassthroughMutedChanged() { trackctl.passthroughMuted = ports_manager.passthroughMuted }
    // }

    // Component.onCompleted: {
    //     muted = ports_manager.muted
    //     passthroughMuted = ports_manager.passthroughMuted
    // }

    function toggle_muted() {
        var n = !muted
        audio_out_ports.forEach((p) => p.set_backend_muted(n))
        midi_out_ports.forEach((p) => p.set_backend_muted(n))
    }

    function toggle_passthroughMuted() {
        var n = !passthroughMuted
        audio_in_ports.forEach((p) => p.set_backend_muted(n))
        midi_in_ports.forEach((p) => p.set_backend_muted(n))
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
                value: output_peak_meter_l.value
                padding: 2
                anchors {
                    left: parent.left
                    right: parent.horizontalCenter
                    verticalCenter: volume_row.verticalCenter
                }

                // Note: dB
                from: -30.0
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
                        color: 'grey'
                        x: (1.0 - output_peak_bar_l.visualPosition) * parent.width
                    }
                }
            }
            ProgressBar {
                id: output_peak_bar_r
                value: output_peak_meter_r.value
                padding: 2
                anchors {
                    left: parent.horizontalCenter
                    right: parent.right
                    verticalCenter: volume_row.verticalCenter
                }

                // Note: dB
                from: -30.0
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
                        color: 'grey'
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
                spacing: -2
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
                Slider {
                    id: volume_slider
                    orientation: Qt.Horizontal
                    width: 85
                    height: 20
                    from: -30.0
                    to: 20.0
                    value: 0.0

                    ToolTip {
                        delay: 1000
                        visible: volume_ma.containsMouse
                        text: 'Playback and monitoring volume.'
                    }
                    MouseArea {
                        id: volume_ma
                        hoverEnabled: true
                        anchors.fill: parent
                        acceptedButtons: Qt.NoButton
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
                value: input_peak_meter_l.value
                padding: 2
                anchors {
                    left: parent.left
                    right: parent.horizontalCenter
                    verticalCenter: passthrough_row.verticalCenter
                }

                // Note: dB
                from: -30.0
                to: 0.0

                AudioLevelMeterModel {
                    id: input_peak_meter_l
                    max_dt: 0.1

                    input: trackctl.audio_in_ports.length > 0 ? trackctl.audio_in_ports[0].peak : 0.0
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
                        color: 'grey'
                        x: parent.width - input_peak_l_bar.visualPosition * parent.width
                    }
                }
            }
            ProgressBar {
                id: input_peak_r_bar
                value: input_peak_meter_r.value
                padding: 2
                anchors {
                    left: parent.horizontalCenter
                    right: parent.right
                    verticalCenter: passthrough_row.verticalCenter
                }

                // Note: dB
                from: -30.0
                to: 0.0

                AudioLevelMeterModel {
                    id: input_peak_meter_r
                    max_dt: 0.1

                    input: trackctl.audio_in_ports.length > 1 ? trackctl.audio_in_ports[1].peak : 0.0
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
                        color: 'grey'
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
                spacing: -2

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
                Slider {
                    id: passthrough_slider
                    orientation: Qt.Horizontal
                    width: 85
                    height: 20
                    from: -30.0
                    to: 20.0
                    value: 0.0

                    ToolTip {
                        delay: 1000
                        visible: passthrough_ma.containsMouse
                        text: 'Passthrough monitoring level.'
                    }
                    MouseArea {
                        id: passthrough_ma
                        hoverEnabled: true
                        anchors.fill: parent
                        acceptedButtons: Qt.NoButton
                    }
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
