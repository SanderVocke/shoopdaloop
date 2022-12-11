import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

// The track control widget displays control buttons to control the
// (loops within a) track.
Item {
    id: trackctl
    width: childrenRect.width
    height: childrenRect.height

    property var port_manager

    signal mute()
    signal unmute()
    property bool muted

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

                    input: trackctl.port_manager ? trackctl.port_manager.outputPeak : 0.0
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

                    value: 1.0

                    // Bidirectional link with the actual backend property
                    onValueChanged: () => { 
                        if (trackctl.port_manager && volume.value != trackctl.port_manager.volume) {
                            trackctl.port_manager.volume = volume.value
                        }
                    }
                    Connections {
                        target: trackctl.port_manager
                        function onVolumeChanged (v) { volume.value = v }
                    }

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

                    input: trackctl.port_manager ? trackctl.port_manager.inputPeak : 0.0
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

                    value: 1.0

                    // Bidirectional link with the actual backend property
                    onValueChanged: () => { 
                        if (trackctl.port_manager && passthrough.value != trackctl.port_manager.passthrough) {
                            trackctl.port_manager.passthrough = passthrough.value
                        }
                    }
                    Connections {
                        target: trackctl.port_manager
                        function onPassthroughChanged (v) { passthrough.value = v }
                    }

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
                    name: trackctl.port_manager.muted ? 'volume-mute' : 'volume-high'
                    color: trackctl.port_manager.muted ? 'grey' : Material.foreground
                }
                onClicked: { trackctl.port_manager.muted = !trackctl.port_manager.muted }

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
                    color: trackctl.port_manager.passthroughMuted ?
                        'grey' : Material.foreground
                }
                onClicked: { trackctl.port_manager.passthroughMuted = !trackctl.port_manager.passthroughMuted }

                hoverEnabled: true
                ToolTip.delay: 1000
                ToolTip.timeout: 5000
                ToolTip.visible: hovered
                ToolTip.text: "En-/disable monitoring. Monitoring level matches track volume."
            }
        }
    }
}
