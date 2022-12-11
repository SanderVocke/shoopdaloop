import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

// The track control widget displays control buttons to control the
// (loops within a) track.
Item {
    id: trackctl
    width: childrenRect.width
    height: childrenRect.height

    property var dry_left_port_manager
    property var dry_right_port_manager
    property var wet_left_port_manager
    property var wet_right_port_manager

    property real max_output_peak: Math.max(
        wet_left_port_manager ? wet_left_port_manager.outputPeak : 0.0,
        wet_right_port_manager ? wet_right_port_manager.outputPeak : 0.0
    )

    property real max_input_peak: Math.max(
        dry_left_port_manager ? dry_left_port_manager.inputPeak : 0.0,
        dry_right_port_manager ? dry_right_port_manager.inputPeak : 0.0
    )

    property alias volume: volume.value
    property alias passthrough: passthrough.value
    property bool muted: wet_left_port_manager ? wet_left_port_manager.muted : true
    property bool passthroughMuted: dry_left_port_manager ? dry_left_port_manager.passthroughMuted : true

    function push_volume(target) {
        if (target && target.volume != volume) { target.volume = volume }
    }

    function push_passthrough(target) {
        if (target && target.passthrough != passthrough) { target.passthrough = passthrough }
    }

    function toggle_muted() {
        var n = !wet_left_port_manager.muted
        wet_left_port_manager.muted = n
        wet_right_port_manager.muted = n
    }

    function toggle_passthroughMuted() {
        var n = !dry_left_port_manager.passthroughMuted
        dry_left_port_manager.passthroughMuted = n
        dry_right_port_manager.passthroughMuted = n
    }

    onVolumeChanged: {
        push_volume(wet_left_port_manager)
        push_volume(wet_right_port_manager)
    }

    onPassthroughChanged: {
        push_passthrough(dry_left_port_manager)
        push_passthrough(dry_right_port_manager)
    }

    Connections {
        target: wet_left_port_manager || null
        function onVolumeChanged() { volume = wet_left_port_manager.volume }
    }

    Connections {
        target: dry_left_port_manager || null
        function onPassthroughChanged() { passthrough = wet_left_port_manager.passthrough }
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

                VUMeterModel {
                    id: output_peak_meter
                    charge_rate: 1000
                    discharge_rate: 3
                    max_dt: 0.1

                    input: trackctl.max_output_peak
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
                    anchors.verticalCenter: volume.verticalCenter
                }
                Slider {
                    id: volume
                    orientation: Qt.Horizontal
                    width: 90
                    height: 20
                    from: 0.0
                    to: 1.0

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

                VUMeterModel {
                    id: input_peak_meter
                    charge_rate: 1000
                    discharge_rate: 3
                    max_dt: 0.1

                    input: trackctl.max_input_peak
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
                    anchors.verticalCenter: passthrough.verticalCenter
                }
                Slider {
                    id: passthrough
                    orientation: Qt.Horizontal
                    width: 90
                    height: 20
                    from: 0.0
                    to: 1.0

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
