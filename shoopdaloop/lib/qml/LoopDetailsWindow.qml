import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import '../mode_helpers.js' as ModeHelpers

ApplicationWindow {
        property var loop
        id: window

        width: 500
        height: 400
        minimumWidth: 200
        minimumHeight: 50
        
        Material.theme: Material.Dark

        Column {
            anchors.margins: 5
            anchors.centerIn: parent
            spacing: 5
            anchors.fill: parent

            Item {
                anchors.left: parent.left
                anchors.right: parent.right
                height: 400
                id: item

                LoopContentWidget {
                    id: waveform
                    min_db: -50.0
                    loop: window.loop
                    anchors.fill: parent

                    Connections {
                        target: window
                        function onVisibleChanged (visible) {
                            if (visible) { waveform.update_data() }
                        }
                    }

                    Connections {
                        target: loop
                        
                        // TODO the triggering
                        function maybe_update() {
                            if (window.visible &&
                                !ModeHelpers.is_recording_mode(loop.mode)) {
                                    waveform.update_data()
                                }
                        }
                        function onStateChanged() {
                            maybe_update()
                            waveform.recording = ModeHelpers.is_recording_mode(loop.mode)
                        }
                        function onLengthChanged() { maybe_update() }
                    }
                }
            }
        }
    }