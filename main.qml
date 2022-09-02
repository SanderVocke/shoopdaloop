import QtQuick 2.15
import QtQuick.Controls 2.15

ApplicationWindow {
    visible: true
    width: 1000
    height: 800
    title: "ShoopDaLoop"

    component LoopProgressIndicator : ProgressBar {
        property double length
        property double pos

        value: pos / length
    }

    Column {
        LoopProgressIndicator {
            length: loop_manager.length
            pos: loop_manager.pos
        }
        LoopProgressIndicator {
            length: 100.0
            pos: 89.0
        }

        Text {
            text: "Hello World"
            font.pixelSize: 24
        }
    }

}