import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3
import ShoopDaLoop.PythonLogger

import '../mode_helpers.js' as ModeHelpers

ApplicationWindow {
    property var loop_widget
    property var maybe_backend_loop : loop_widget.maybe_backend_loop
    property var master_loop_widget

    property PythonLogger logger : PythonLogger { name: default_logger.name + '.LoopDetails' }
    
    id: root

    width: 800
    height: 250
    minimumWidth: 200
    minimumHeight: 50
    
    Material.theme: Material.Dark

    Loader {
        active: root.maybe_backend_loop
        sourceComponent: Component {
            id: loop_content_widget
            LoopContentWidget {
                visible: root.visible
                id: waveform
                anchors.margins: 5
                loop: root.loop_widget
                master_loop: root.master_loop_widget
                logger: root.logger
            }
        }
        anchors.fill: parent
    }

    Loader {
        active: root.maybe_composite_loop
        sourceComponent: Label {
            text: "Detailed editing for composite loops is not yet supported. Stay tuned!"
            color: Material.foreground
        }
    }
}