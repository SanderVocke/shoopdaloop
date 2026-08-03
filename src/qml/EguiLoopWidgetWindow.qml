import QtQuick 6.6
import ShoopDaLoop.Rust

ShoopApplicationWindow {
    id: root

    property var loopWidget: null
    readonly property string loopName: loopWidget ? loopWidget.name : "Loop"
    readonly property real loopPosition: loopWidget && loopWidget.length > 0
        ? loopWidget.position / loopWidget.length
        : 0.0
    readonly property bool loopPlaying: loopWidget
        && (loopWidget.mode === ShoopRustConstants.LoopMode.Playing
            || loopWidget.mode === ShoopRustConstants.LoopMode.PlayingDryThroughWet)

    function updateCanvas() {
        canvas.setLoopState(loopName, loopPosition, loopPlaying)
    }

    onLoopNameChanged: updateCanvas()
    onLoopPositionChanged: updateCanvas()
    onLoopPlayingChanged: updateCanvas()

    title: loopName + " — egui prototype"
    width: 620
    height: 150
    minimumWidth: 360
    minimumHeight: 100

    ShoopEguiLoopWidget {
        id: canvas
        anchors.fill: parent
        focus: true

        Component.onCompleted: root.updateCanvas()
        onPlayClicked: if (root.loopWidget) root.loopWidget.on_play_clicked()
        onStopClicked: if (root.loopWidget) root.loopWidget.on_stop_clicked()
    }
}
