import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

// The track control widget displays control buttons to control the
// (loops within a) track.
Item {
    id: trackctl
    width: childrenRect.width
    height: childrenRect.height

    property list backend_port_states

    signal mute()
    signal unmute()
    property bool muted
    property alias volume: volume.value
    property alias pan: pan.value
    property bool passthrough_enabled: true
    property real passthrough: passthrough_enabled ? volume.value : 0.0

    Column {
        spacing: 0
        width: 100

        Row {
            spacing: -2

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

                property real externalValue : backend_port_states.length > 0 ?
                    backend_port_states[0].volume : 1.0

                value: 1.0
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
        Row {
            spacing: -2

            MaterialDesignIcon {
                size: 15
                name: 'unfold-more-vertical'
                color: Material.foreground
                anchors.verticalCenter: pan.verticalCenter
            }
            Slider {
                id: pan
                orientation: Qt.Horizontal
                width: 90
                height: 20
                from: -1.0
                to: 1.0
                value: 0.0
                ToolTip {
                    delay: 1000
                    visible: pan_ma.containsMouse
                    text: 'Pan control. Right-click to center.'
                }
                MouseArea {
                    id: pan_ma
                    hoverEnabled: true
                    anchors.fill: parent
                    acceptedButtons: Qt.RightButton
                    onClicked: (ev) => { pan.value = 0.0; }
                }
            }
        }

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
                onClicked: { if(trackctl.muted) {trackctl.unmute()} else {trackctl.mute()} }

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
                    color: trackctl.passthrough_enabled ? Material.foreground : 'grey'
                }
                onClicked: { trackctl.passthrough_enabled = !trackctl.passthrough_enabled }

                hoverEnabled: true
                ToolTip.delay: 1000
                ToolTip.timeout: 5000
                ToolTip.visible: hovered
                ToolTip.text: "En-/disable monitoring. Monitoring level matches track volume."
            }
        }
    }
}
