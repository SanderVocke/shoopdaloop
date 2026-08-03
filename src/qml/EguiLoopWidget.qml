import QtQuick 6.6
import ShoopDaLoop.Rust

Item {
    id: root

    property var loopWidget: null
    property bool canvasReady: false

    function updateCanvas() {
        if (!canvasReady) {
            return
        }
        const position = loopWidget && loopWidget.length > 0
            ? loopWidget.position / loopWidget.length
            : 0.0
        const playing = loopWidget
            && (loopWidget.mode === ShoopRustConstants.LoopMode.Playing
                || loopWidget.mode === ShoopRustConstants.LoopMode.PlayingDryThroughWet)
        canvas.setLoopState(loopWidget ? loopWidget.name : "Loop", position, playing)
    }

    onLoopWidgetChanged: updateCanvas()

    Connections {
        target: root.loopWidget
        ignoreUnknownSignals: true

        function onNameChanged() { root.updateCanvas() }
        function onPositionChanged() { root.updateCanvas() }
        function onLengthChanged() { root.updateCanvas() }
        function onModeChanged() { root.updateCanvas() }
    }

    ShoopEguiLoopWidget {
        id: canvas
        anchors.fill: parent
        focus: true

        Component.onCompleted: {
            root.canvasReady = true
            root.updateCanvas()
        }
        onPlayClicked: if (root.loopWidget) root.loopWidget.on_play_clicked()
        onStopClicked: if (root.loopWidget) root.loopWidget.on_stop_clicked()
    }
}
