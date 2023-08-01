import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3
import ShoopDaLoop.PythonLogger

import '../mode_helpers.js' as ModeHelpers

ApplicationWindow {
    property var loop
    property var master_loop

    property PythonLogger logger : PythonLogger { name: default_logger.name + '.LoopDetails' }
    
    id: root

    width: 800
    height: 250
    minimumWidth: 200
    minimumHeight: 50
    
    Material.theme: Material.Dark

    LoopContentWidget {
        visible: root.visible
        id: waveform
        anchors.fill: parent
        anchors.margins: 5
        loop: root.loop
        master_loop: root.master_loop
        logger: root.logger
    }
}