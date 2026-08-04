import QtQuick 6.6
import ShoopDaLoop.Rust

Item {
    id: root

    property var loopWidget: null
    property bool canvasReady: false

    EguiLoopStateBridge {
        id: stateBridge
        loopWidget: root.loopWidget
        loopStateSink: state => {
            if (root.canvasReady) {
                canvas.setLoopState(
                    state.name,
                    state.position,
                    state.mode,
                    state.nextMode,
                    state.nextTransitionDelay,
                    state.empty,
                    state.regularComposite,
                    state.scriptComposite,
                    state.sync,
                    state.targeted,
                    state.selected,
                    state.selectedCompositeKind,
                    state.showGain,
                    state.gain,
                    state.playAfterRecord
                )
            }
        }
        peakStateSink: (stereo, peakLeftDb, peakRightDb, midiActivity) => {
            if (root.canvasReady) {
                canvas.setPeakState(stereo, peakLeftDb, peakRightDb, midiActivity)
            }
        }
    }

    ShoopEguiLoopWidget {
        id: canvas
        anchors.fill: parent
        focus: true

        Component.onCompleted: {
            root.canvasReady = true
            stateBridge.updateLoopState()
            stateBridge.updatePeakState()
        }
        onIconClicked: if (root.loopWidget) root.loopWidget.on_state_icon_clicked()
        onIconDoubleClicked: if (root.loopWidget) root.loopWidget.on_state_icon_double_clicked()
        onPlayClicked: if (root.loopWidget) root.loopWidget.on_play_clicked()
        onRecordClicked: if (root.loopWidget) root.loopWidget.on_record_clicked()
        onStopClicked: if (root.loopWidget) root.loopWidget.on_stop_clicked()
        onGainChanged: (value) => {
            if (root.loopWidget) {
                root.loopWidget.set_gain_fader(value)
            }
        }
    }
}
