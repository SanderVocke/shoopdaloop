import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import '../mode_helpers.js' as ModeHelpers

ApplicationWindow {
        property var loop
        id: window

        width: 500
        height: 400
        minimumWidth: width
        maximumWidth: width
        minimumHeight: height
        maximumHeight: height
        
        Material.theme: Material.Dark

        Column {
            anchors.margins: 5
            anchors.centerIn: parent
            spacing: 5
            anchors.fill: parent

            Item {
                anchors.left: parent.left
                anchors.right: parent.right
                height: 100
                id: item

                LoopContentWidget {
                    id: waveform
                    waveform_data_max: 1.0
                    min_db: -50.0
                    loop: window.loop
                    samples_per_waveform_pixel: loop.length / width
                    length_samples: loop.length
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