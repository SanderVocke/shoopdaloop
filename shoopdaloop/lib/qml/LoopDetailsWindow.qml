import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import '../mode_helpers.js' as ModeHelpers

ApplicationWindow {
    property var loop
    property var master_loop
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
        anchors.margins: 5
        loop: window.loop
        master_loop: window.master_loop
    }
}