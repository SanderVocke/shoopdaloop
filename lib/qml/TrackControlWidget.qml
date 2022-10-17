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
    signal playLiveFx()

    property bool muted

    Column {
        spacing: 0
        width: 100

        Slider {
            id: volume
            x: 0
            y: 0

            orientation: Qt.Horizontal
            width: parent.width
            height: 20
            from: 0.0
            to: 1.0
            value: 1.0
        }
        Slider {
            id: pan
            x: 0
            y: 0

            orientation: Qt.Horizontal
            width: parent.width
            height: 20
            from: -1.0
            to: 1.0
            value: 0.0
        }

        Grid {
            width: childrenRect.width
            height: childrenRect.height
            x: (parent.width - width) / 2

            spacing: 2
            columns: 3

            Button {
                id : record
                width: 30
                height: 30
                MaterialDesignIcon {
                    size: parent.width - 10
                    anchors.centerIn: parent
                    name: 'record'
                    color: 'red'
                }
                onClicked: { trackctl.record() }
            }
            Button {
                id : recordN
                width: 30
                height: 30
                IconWithText {
                    size: parent.width - 10
                    anchors.centerIn: parent
                    name: 'record'
                    color: 'red'
                    text_color: Material.foreground
                    text: "1"
                    font.pixelSize: size / 2.0
                }
                onClicked: { trackctl.recordNCycles(1) }
            }
            Button {
                id : recordfx
                width: 30
                height: 30
                IconWithText {
                    size: parent.width - 10
                    anchors.centerIn: parent
                    name: 'record'
                    color: 'orange'
                    text_color: Material.foreground
                    text: "FX"
                }
                onClicked: { trackctl.recordFx() }
            }
            Button {
                id : pause
                width: 30
                height: 30
                MaterialDesignIcon {
                    size: parent.width - 10
                    anchors.centerIn: parent
                    name: 'pause'
                    color: Material.foreground
                }
                onClicked: { trackctl.playLiveFx() }
            }
            Button {
                id : play
                width: 30
                height: 30
                MaterialDesignIcon {
                    size: parent.width - 10
                    anchors.centerIn: parent
                    name: 'play'
                    color: 'green'
                }
                onClicked: { trackctl.play() }
            }
            Button {
                id : playlivefx
                width: 30
                height: 30
                IconWithText {
                    size: parent.width - 10
                    anchors.centerIn: parent
                    name: 'play'
                    color: 'orange'
                    text_color: Material.foreground
                    text: "FX"
                }
                onClicked: { trackctl.playLiveFx() }
            }
            Button {
                id : mute
                width: 30
                height: 30
                MaterialDesignIcon {
                    size: parent.width - 10
                    anchors.centerIn: parent
                    name: trackctl.muted ? 'volume-mute' : 'volume-high'
                    color: trackctl.muted ? 'grey' : Material.foreground
                }
                onClicked: { if(trackctl.muted) {trackctl.unmute()} else {trackctl.mute()} }
            }
        }
    }
}
