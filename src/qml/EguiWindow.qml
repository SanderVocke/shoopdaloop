import QtQuick 6.6
import ShoopDaLoop.Rust

ShoopApplicationWindow {
    id: root

    property var tracks: []
    property var trackSnapshots: []
    property var loopBridges: []
    property bool canvasReady: false
    property bool initialized: false

    title: "ShoopDaLoop — egui prototype"
    width: 900
    height: 600
    minimumWidth: 360
    minimumHeight: 200

    function loopWidget(trackIndex, loopIndex) {
        const track = trackSnapshots[trackIndex]
        return track && track.loops ? track.loops[loopIndex] : null
    }

    function initializeCanvas() {
        if (!canvasReady || initialized) {
            return
        }
        initialized = true

        const snapshots = []
        tracks.filter(track => track).forEach(track => {
            snapshots.push({
                name: track.name,
                loops: Array.from(track.loops).filter(loop => loop && loop.objectName === "Qml.LoopWidget")
            })
        })
        trackSnapshots = snapshots

        snapshots.forEach((track, trackIndex) => {
            canvas.setTrack(trackIndex, track.name, track.loops.length)
            track.loops.forEach((loop, loopIndex) => {
                const bridge = loopBridgeFactory.createObject(canvas, {
                    loopWidget: loop,
                    loopStateSink: state => canvas.setLoopState(
                        trackIndex,
                        loopIndex,
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
                    ),
                    peakStateSink: (stereo, peakLeftDb, peakRightDb, midiActivity) => canvas.setPeakState(
                        trackIndex,
                        loopIndex,
                        stereo,
                        peakLeftDb,
                        peakRightDb,
                        midiActivity
                    )
                })
                if (!bridge) {
                    throw new Error("EguiWindow: Failed to create loop state bridge")
                }
                loopBridges.push(bridge)
            })
        })
    }

    Component {
        id: loopBridgeFactory
        EguiLoopStateBridge {}
    }

    ShoopEguiWindow {
        id: canvas
        anchors.fill: parent
        focus: true

        Component.onCompleted: {
            root.canvasReady = true
            root.initializeCanvas()
        }

        onIconClicked: (trackIndex, loopIndex) => {
            const loop = root.loopWidget(trackIndex, loopIndex)
            if (loop) loop.on_state_icon_clicked()
        }
        onIconDoubleClicked: (trackIndex, loopIndex) => {
            const loop = root.loopWidget(trackIndex, loopIndex)
            if (loop) loop.on_state_icon_double_clicked()
        }
        onPlayClicked: (trackIndex, loopIndex) => {
            const loop = root.loopWidget(trackIndex, loopIndex)
            if (loop) loop.on_play_clicked()
        }
        onRecordClicked: (trackIndex, loopIndex) => {
            const loop = root.loopWidget(trackIndex, loopIndex)
            if (loop) loop.on_record_clicked()
        }
        onStopClicked: (trackIndex, loopIndex) => {
            const loop = root.loopWidget(trackIndex, loopIndex)
            if (loop) loop.on_stop_clicked()
        }
        onGainChanged: (trackIndex, loopIndex, value) => {
            const loop = root.loopWidget(trackIndex, loopIndex)
            if (loop) loop.set_gain_fader(value)
        }
    }

    Component.onCompleted: initializeCanvas()
    onClosing: destroy()
}
