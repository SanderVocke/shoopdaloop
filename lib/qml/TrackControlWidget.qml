import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

// The track control widget displays control buttons to control the
// (loops within a) track.
Item {
    id: trackctl
    width: childrenRect.width
    height: childrenRect.height

    signal pause()
    signal mute()
    signal unmute()
    signal record()
    signal recordFx()
    signal recordNCycles(int n)
    signal play()
    signal playDry()
    signal playLiveFx()

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
                id : record
                width: 23
                height: 30
                MaterialDesignIcon {
                    size: parent.width - 3
                    anchors.centerIn: parent
                    name: 'record'
                    color: 'red'
                }
                onClicked: { trackctl.record() }

                hoverEnabled: true
                ToolTip.delay: 1000
                ToolTip.timeout: 5000
                ToolTip.visible: hovered
                ToolTip.text: "Trigger/stop recording. For master loop, starts immediately. For others, start/stop synced to next master loop cycle."
            }
            Button {
                id : recordN
                property int n: 1
                width: 23
                height: 30
                IconWithText {
                    size: parent.width - 3
                    anchors.centerIn: parent
                    name: 'record'
                    color: 'red'
                    text_color: Material.foreground
                    text: recordN.n.toString()
                    font.pixelSize: size / 2.0
                }
                onClicked: { trackctl.recordNCycles(n) }
                onPressAndHold: { menu.popup() }

                hoverEnabled: true
                ToolTip.delay: 1000
                ToolTip.timeout: 5000
                ToolTip.visible: hovered
                ToolTip.text: "Trigger fixed-length recording. Length (number shown) is the amount of master loop cycles to record. Press and hold this button to change this number."

                // TODO: editable text box instead of fixed options
                Menu {
                    id: menu
                    title: 'Select # of cycles'
                    MenuItem {
                        text: "1"
                        onClicked: () => { recordN.n = 1 }
                    }
                    MenuItem {
                        text: "2"
                        onClicked: () => { recordN.n = 2 }
                    }
                    MenuItem {
                        text: "4"
                        onClicked: () => { recordN.n = 4 }
                    }
                    MenuItem {
                        text: "8"
                        onClicked: () => { recordN.n = 8 }
                    }
                    MenuItem {
                        text: "16"
                        onClicked: () => { recordN.n = 16 }
                    }
                }
            }
            
            Button {
                id : recordfx
                width: 23
                height: 30
                IconWithText {
                    size: parent.width - 3
                    anchors.centerIn: parent
                    name: 'record'
                    color: 'orange'
                    text_color: Material.foreground
                    text: "FX"
                }
                onClicked: { trackctl.recordFx() }

                hoverEnabled: true
                ToolTip.delay: 1000
                ToolTip.timeout: 5000
                ToolTip.visible: hovered
                ToolTip.text: "Trigger FX re-record. This will play the full dry loop once with live FX, recording the result for wet playback."
            }
            Button {
                id : playlivefx
                width: 23
                height: 30
                IconWithText {
                    size: parent.width - 3
                    anchors.centerIn: parent
                    name: 'play'
                    color: 'orange'
                    text_color: Material.foreground
                    text: "FX"
                }
                onClicked: { trackctl.playLiveFx() }

                hoverEnabled: true
                ToolTip.delay: 1000
                ToolTip.timeout: 5000
                ToolTip.visible: hovered
                ToolTip.text: "Play dry recording through live effects. Allows hearing FX changes on-the-fly."
            }
            Button {
                id : play
                width: 23
                height: 30
                MaterialDesignIcon {
                    size: parent.width - 3
                    anchors.centerIn: parent
                    name: 'play'
                    color: 'green'
                }
                onClicked: { trackctl.play() }

                hoverEnabled: true
                ToolTip.delay: 1000
                ToolTip.timeout: 5000
                ToolTip.visible: hovered
                ToolTip.text: "Play wet recording."
            }
            Button {
                id : pause
                width: 23
                height: 30
                MaterialDesignIcon {
                    size: parent.width - 3
                    anchors.centerIn: parent
                    name: 'pause'
                    color: Material.foreground
                }
                onClicked: { trackctl.pause() }

                hoverEnabled: true
                ToolTip.delay: 1000
                ToolTip.timeout: 5000
                ToolTip.visible: hovered
                ToolTip.text: "(Un)pause."
            }
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
