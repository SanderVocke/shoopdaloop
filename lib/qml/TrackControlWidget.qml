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
    signal unpause()
    signal mute()
    signal unmute()
    signal record()

    property bool paused
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

        Column {
            width: childrenRect.width
            height: childrenRect.height
            x: (parent.width - width) / 2

            spacing: 2

            Row {
                spacing: 2

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
                    id : pause
                    width: 30
                    height: 30
                    MaterialDesignIcon {
                        size: parent.width - 10
                        anchors.centerIn: parent
                        name: trackctl.paused ? 'play' : 'pause'
                        color: Material.foreground
                    }
                    onClicked: { if(pause.paused) {trackctl.unpause()} else {trackctl.pause()} }
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
                    onClicked: { if(mute.muted) {trackctl.unmute()} else {trackctl.mute()} }
                }
            }

            Rectangle {
                width: childrenRect.width
                height: childrenRect.height
                color: 'grey'

                Column {
                    width: childrenRect.width
                    height: childrenRect.height
                    spacing: 0

                    Text {
                        text: "fx proc"
                        color: Material.foreground
                        width: 50
                        topPadding: 2
                        leftPadding: 5
                        bottomPadding: 0
                        font.pixelSize: 15
                    }

                    Row {
                        width: childrenRect.width
                        height: childrenRect.height
                        spacing: 2

                        Button {
                            id : fx_off
                            width: 30
                            height: 30
                            MaterialDesignIcon {
                                size: parent.width - 10
                                anchors.centerIn: parent
                                name: 'close'
                                color: Material.foreground
                            }
                        }
                        Button {
                            id : fx_live
                            width: 30
                            height: 30
                            MaterialDesignIcon {
                                size: parent.width - 10
                                anchors.centerIn: parent
                                name: 'record'
                                color: Material.foreground
                            }
                        }
                        Button {
                            id : fx_cached
                            width: 30
                            height: 30
                            MaterialDesignIcon {
                                size: parent.width - 10
                                anchors.centerIn: parent
                                name: 'sd'
                                color: Material.foreground
                            }
                        }
                    }
                }
            }
        }
    }
}
