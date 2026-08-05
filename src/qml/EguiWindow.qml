import QtQuick 6.6
import ShoopDaLoop.Rust

ShoopApplicationWindow {
    id: root

    property var tracks: []
    property var backend: null
    property var appControls: null
    property var trackSnapshots: []
    property var loopBridges: []
    property var trackBridges: []
    property bool canvasReady: false
    property bool initialized: false
    property alias detailsStateBridge: detailsBridge

    title: "ShoopDaLoop — egui prototype"
    width: 900
    height: 600
    minimumWidth: 360
    minimumHeight: 200

    function loopWidget(trackIndex, loopIndex) {
        const track = trackSnapshots[trackIndex]
        return track && track.loops ? track.loops[loopIndex] : null
    }

    function trackWidget(trackIndex) {
        return tracks[trackIndex] || null
    }

    function trackControl(trackIndex) {
        const track = trackWidget(trackIndex)
        return track ? track.control_widget : null
    }

    function handleTrackNameChanged(trackIndex, name) {
        const track = trackWidget(trackIndex)
        if (track) track.name = name
    }

    function handleTrackOutputGainChanged(trackIndex, value) {
        const control = trackControl(trackIndex)
        if (control) control.gain_dB = value
    }

    function handleTrackOutputBalanceChanged(trackIndex, value) {
        const control = trackControl(trackIndex)
        if (control) control.set_balance(value)
    }

    function handleTrackOutputMuteChanged(trackIndex, value) {
        const control = trackControl(trackIndex)
        if (control) control.set_mute(value)
    }

    function handleTrackInputGainChanged(trackIndex, value) {
        const control = trackControl(trackIndex)
        if (control) control.input_gain_dB = value
    }

    function handleTrackInputBalanceChanged(trackIndex, value) {
        const control = trackControl(trackIndex)
        if (control) control.input_balance = value
    }

    function handleTrackInputMonitoringChanged(trackIndex, value) {
        const control = trackControl(trackIndex)
        if (control) control.set_monitor(value)
    }

    function handleGlobalStopAll() {
        if (appControls) appControls.stop_all()
    }

    function handleGlobalDeselectAll() {
        if (appControls) appControls.deselect_all()
    }

    function handleGlobalClearRecordings(includeSync) {
        if (appControls) appControls.request_clear_recordings(includeSync)
    }

    function handleGlobalClearAll(includeSync) {
        if (appControls) appControls.request_clear_all(includeSync)
    }

    function handleDefaultRecordingActionChanged(value) {
        if (appControls) appControls.set_default_recording_action(value)
    }

    function handlePlayAfterRecordChanged(value) {
        if (appControls) appControls.set_play_after_record(value)
    }

    function handleSyncChanged(value) {
        if (appControls) appControls.set_sync(value)
    }

    function handleSoloChanged(value) {
        if (appControls) appControls.set_solo(value)
    }

    function handleApplyNCyclesChanged(value) {
        if (appControls) appControls.set_apply_n_cycles(value)
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
            const trackBridge = trackBridgeFactory.createObject(canvas, {
                trackWidget: tracks[trackIndex],
                stateSink: state => {
                    canvas.setTrack(trackIndex, state.name, track.loops.length)
                    canvas.setTrackControlState(
                        trackIndex,
                        state.hasOutput,
                        state.hasOutputAudio,
                        state.outputStereo,
                        state.outputGainDb,
                        state.outputBalance,
                        state.outputMuted,
                        state.outputPeakLeftDb,
                        state.outputPeakRightDb,
                        state.outputMidiActivity,
                        state.hasInput,
                        state.hasInputAudio,
                        state.inputStereo,
                        state.inputGainDb,
                        state.inputBalance,
                        state.inputMonitoring,
                        state.inputPeakLeftDb,
                        state.inputPeakRightDb,
                        state.inputMidiActivity
                    )
                }
            })
            if (!trackBridge) {
                throw new Error("EguiWindow: Failed to create track state bridge")
            }
            trackBridges.push(trackBridge)

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

    Component {
        id: trackBridgeFactory
        EguiTrackStateBridge {}
    }

    EguiGlobalStateBridge {
        backend: root.backend
        stateSink: state => canvas.setGlobalState(
            state.version,
            state.dspLoadPercent,
            state.xruns,
            state.bufferSize,
            state.sampleRate,
            state.defaultRecordingAction,
            state.playAfterRecord,
            state.sync,
            state.solo,
            state.applyNCycles
        )
    }

    SelectedLoops {
        id: selectedLoopsLookup
    }

    EguiDetailsStateBridge {
        id: detailsBridge
        canvas: canvas
        selectedLoops: Array.from(selectedLoopsLookup.loops).filter(loop => loop)
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
        onTrackNameChanged: (trackIndex, name) => root.handleTrackNameChanged(trackIndex, name)
        onTrackOutputGainChanged: (trackIndex, value) => root.handleTrackOutputGainChanged(trackIndex, value)
        onTrackOutputBalanceChanged: (trackIndex, value) => root.handleTrackOutputBalanceChanged(trackIndex, value)
        onTrackOutputMuteChanged: (trackIndex, value) => root.handleTrackOutputMuteChanged(trackIndex, value)
        onTrackInputGainChanged: (trackIndex, value) => root.handleTrackInputGainChanged(trackIndex, value)
        onTrackInputBalanceChanged: (trackIndex, value) => root.handleTrackInputBalanceChanged(trackIndex, value)
        onTrackInputMonitoringChanged: (trackIndex, value) => root.handleTrackInputMonitoringChanged(trackIndex, value)

        onGlobalStopAll: root.handleGlobalStopAll()
        onGlobalDeselectAll: root.handleGlobalDeselectAll()
        onGlobalClearRecordings: includeSync => root.handleGlobalClearRecordings(includeSync)
        onGlobalClearAll: includeSync => root.handleGlobalClearAll(includeSync)
        onDefaultRecordingActionChanged: value => root.handleDefaultRecordingActionChanged(value)
        onPlayAfterRecordChanged: value => root.handlePlayAfterRecordChanged(value)
        onSyncChanged: value => root.handleSyncChanged(value)
        onSoloChanged: value => root.handleSoloChanged(value)
        onApplyNCyclesChanged: value => root.handleApplyNCyclesChanged(value)
    }

    Component.onCompleted: initializeCanvas()
    onClosing: destroy()
}
