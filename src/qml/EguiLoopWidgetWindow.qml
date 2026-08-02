import QtQuick 6.6
import EguiCxxQt 1.0

ShoopApplicationWindow {
    id: root

    property string loopName: "Loop"

    title: loopName + " — egui prototype"
    width: 620
    height: 150
    minimumWidth: 360
    minimumHeight: 100

    EguiCanvas {
        anchors.fill: parent
        uiType: "loop-widget"
        focus: true
    }
}
