import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

// The track control widget displays control buttons to control the
// (loops within a) track.
Item {
    id: trackctl
    width: childrenRect.width
    height: childrenRect.height

    property var ports_manager

    property alias volume_dB: volume_slider.value
    property alias passthrough_dB: passthrough_slider.value
    property alias volume_dB_min: volume_slider.from
    property alias passthrough_dB_min: passthrough_slider.from
    property bool muted: ports_manager.muted
    property bool passthroughMuted: ports_manager.passthroughMuted

    LinearDbConversion {
        id: convert_volume
    }

    LinearDbConversion {
        id: convert_passthrough
    }

    function push_volume(target) {
        convert_volume.dB = volume_dB
        var v = convert_volume.linear
        if (target && target.volume != v) { target.volume = v }
    }

    function push_passthrough(target) {
        convert_passthrough.dB = passthrough_dB
        var v = convert_passthrough.linear
        if (target && target.passthrough != v) { target.passthrough = v }
    }

    onVolume_dBChanged: {
        push_volume(ports_manager)
    }

    onPassthrough_dBChanged: {
        push_passthrough(ports_manager)
    }

    Connections {
        target: ports_manager || null
        function onVolumeChanged() {
            convert_volume.linear = ports_manager.volume
            volume_dB = convert_volume.dB
        }
        function onPassthroughChanged() {
            convert_passthrough.linear = ports_manager.passthrough
            passthrough_dB = convert_passthrough.dB
        }
    }

    Connections {
        target: ports_manager
        function onMutedChanged() { trackctl.muted = ports_manager.muted }
        function onPassthroughMutedChanged() { trackctl.passthroughMuted = ports_manager.passthroughMuted }
    }

    Component.onCompleted: {
        muted = ports_manager.muted
        passthroughMuted = ports_manager.passthroughMuted
    }

    function toggle_muted() {
        var n = !ports_manager.muted
        ports_manager.muted = n
        ports_manager.muted = n
    }

    function toggle_passthroughMuted() {
        var n = !ports_manager.passthroughMuted
        ports_manager.passthroughMuted = n
        ports_manager.passthroughMuted = n
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
                id: output_peak_bar
                value: output_peak_meter.value
                padding: 2
                anchors {
                    left: parent.left
                    right: parent.right
                    verticalCenter: volume_row.verticalCenter
                }

                // Note: dB
                from: -60.0
                to: 0.0

                AudioLevelMeterModel {
                    id: output_peak_meter
                    max_dt: 0.1

                    input: trackctl.ports_manager.outputPeak
                }

                background: Rectangle {
                    implicitWidth: 50
                    implicitHeight: 16
                    color: Material.background
                    radius: 3
                }

                contentItem: Item {
                    implicitWidth: 50
                    implicitHeight: 16

                    Rectangle {
                        width: output_peak_bar.visualPosition * parent.width
                        height: parent.height
                        radius: 2
                        color: 'grey'
                    }
                }
            }
            Row {
                spacing: -2
                id: volume_row

                MaterialDesignIcon {
                    size: 15
                    name: 'volume-high'
                    color: Material.foreground
                    anchors.verticalCenter: volume_slider.verticalCenter
                }
                Slider {
                    id: volume_slider
                    orientation: Qt.Horizontal
                    width: 90
                    height: 20
                    from: -60.0
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
                id: input_peak_bar
                value: input_peak_meter.value
                padding: 2
                anchors {
                    left: parent.left
                    right: parent.right
                    verticalCenter: passthrough_row.verticalCenter
                }

                // Note: dB
                from: -60.0
                to: 0.0

                AudioLevelMeterModel {
                    id: input_peak_meter
                    max_dt: 0.1

                    input: trackctl.ports_manager.inputPeak
                }

                background: Rectangle {
                    implicitWidth: 50
                    implicitHeight: 16
                    color: Material.background
                    radius: 3
                }

                contentItem: Item {
                    implicitWidth: 50
                    implicitHeight: 16

                    Rectangle {
                        width: input_peak_bar.visualPosition * parent.width
                        height: parent.height
                        radius: 2
                        color: 'grey'
                    }
                }
            }
            Row {
                id: passthrough_row
                spacing: -2

                MaterialDesignIcon {
                    size: 15
                    name: 'ear-hearing'
                    color: Material.foreground
                    anchors.verticalCenter: passthrough_slider.verticalCenter
                }
                Slider {
                    id: passthrough_slider
                    orientation: Qt.Horizontal
                    width: 90
                    height: 20
                    from: -60.0
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

        Grid {
            width: childrenRect.width
            height: childrenRect.height
            x: (parent.width - width) / 2

            spacing: 2
            columns: 4
            Button {
                id : mute
                width: 23
                height: 30
                MaterialDesignIcon {
                    size: parent.width - 3
                    anchors.centerIn: parent
                    name: trackctl.muted ? 'volume-mute' : 'volume-high'
                    color: trackctl.muted ? 'grey' : Material.foreground
                }
                onClicked: { trackctl.toggle_muted() }

                hoverEnabled: true
                ToolTip.delay: 1000
                ToolTip.timeout: 5000
                ToolTip.visible: hovered
                ToolTip.text: "(Un-)mute."
            }
            Button {
                id : mon
                width: 23
                height: 30

                MaterialDesignIcon {
                    size: parent.width - 3
                    anchors.centerIn: parent
                    name: 'ear-hearing'
                    color: trackctl.passthroughMuted ?
                        'grey' : Material.foreground
                }
                onClicked: { trackctl.toggle_passthroughMuted() }

                hoverEnabled: true
                ToolTip.delay: 1000
                ToolTip.timeout: 5000
                ToolTip.visible: hovered
                ToolTip.text: "En-/disable monitoring. Monitoring level matches track volume."
            }
        }
    }
}
