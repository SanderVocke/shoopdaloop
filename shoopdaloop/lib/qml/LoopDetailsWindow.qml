import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import '../mode_helpers.js' as ModeHelpers

ApplicationWindow {
        property var loop
        id: window

        width: 800
        height: 250
        minimumWidth: 200
        minimumHeight: 50
        
        Material.theme: Material.Dark

        LoopContentWidget {
            visible: window.visible
            id: waveform
            anchors.fill: parent
            //min_db: -50.0
            loop: window.loop

            // Connections {
            //     target: window
            //     function onVisibleChanged (visible) {
            //         if (visible) { waveform.update_data() }
            //     }
            // }

            // Connections {
            //     target: loop
                
            //     // TODO the triggering
            //     function maybe_update() {
            //         if (window.visible &&
            //             !ModeHelpers.is_recording_mode(loop.mode)) {
            //                 waveform.update_data()
            //             }
            //     }
            //     function onStateChanged() {
            //         maybe_update()
            //         waveform.recording = ModeHelpers.is_recording_mode(loop.mode)
            //     }
            //     function onLengthChanged() { maybe_update() }
            // }
        }
    }