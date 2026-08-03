import QtQuick 6.6
import ShoopDaLoop.Rust

Item {
    id: root

    property var loopWidget: null
    property bool canvasReady: false
    readonly property var loopBackend: loopWidget ? loopWidget.maybe_loop : null
    readonly property var loopState: {
        const widget = loopWidget
        const loop = loopBackend
        const composite = widget ? widget.maybe_composite_loop : null
        const length = widget ? widget.length : 0
        const displayPosition = loop && loop.display_position !== undefined
            ? loop.display_position
            : (widget ? widget.position : 0)
        let selectedCompositeKind = 0
        if (widget && widget.is_in_selected_composite_loop
                && widget.single_selected_composite_loop
                && widget.single_selected_composite_loop.maybe_composite_loop) {
            selectedCompositeKind = widget.single_selected_composite_loop.maybe_composite_loop.kind === "regular" ? 1 : 2
        }
        return {
            name: widget ? widget.name : "Loop",
            position: length > 0 ? displayPosition / length : 0.0,
            mode: widget ? widget.mode : ShoopRustConstants.LoopMode.Unknown,
            nextMode: widget ? widget.next_mode : ShoopRustConstants.LoopMode.Unknown,
            nextTransitionDelay: widget ? widget.next_transition_delay : -1,
            empty: !loop || length === 0,
            regularComposite: composite && composite.kind === "regular",
            scriptComposite: composite && composite.kind === "script",
            sync: widget ? widget.is_sync : false,
            targeted: widget ? widget.targeted : false,
            selected: widget ? widget.selected : false,
            selectedCompositeKind: selectedCompositeKind,
            showGain: widget ? widget.descriptor_has_audio && !composite : false,
            gain: widget ? widget.gain_fader : 0.6,
            playAfterRecord: AppRegistries.state_registry.play_after_record_active
        }
    }
    readonly property bool stereo: loopWidget ? loopWidget.is_stereo : false
    readonly property bool midiActivity: loopBackend
        && (loopBackend.display_midi_events_triggered > 0
            || loopBackend.display_midi_notes_active > 0)

    function updateCanvas() {
        if (!canvasReady) {
            return
        }
        const state = loopState
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

    function updatePeakState() {
        if (!canvasReady) {
            return
        }
        canvas.setPeakState(
            stereo,
            stereo ? peakMeterLeft.value : peakMeterOverall.value,
            stereo ? peakMeterRight.value : peakMeterOverall.value,
            midiActivity
        )
    }

    onLoopStateChanged: updateCanvas()
    onStereoChanged: updatePeakState()
    onMidiActivityChanged: updatePeakState()

    AudioLevelMeterModel {
        id: peakMeterLeft
        max_dt: 0.1
        input: root.loopBackend && root.loopBackend.display_peaks
            && root.loopBackend.display_peaks.length >= 1
            ? root.loopBackend.display_peaks[0]
            : 0.0
        onValueChanged: root.updatePeakState()
    }

    AudioLevelMeterModel {
        id: peakMeterRight
        max_dt: 0.1
        input: root.loopBackend && root.loopBackend.display_peaks
            && root.loopBackend.display_peaks.length >= 2
            ? root.loopBackend.display_peaks[1]
            : 0.0
        onValueChanged: root.updatePeakState()
    }

    AudioLevelMeterModel {
        id: peakMeterOverall
        max_dt: 0.1
        input: root.loopBackend && root.loopBackend.display_peaks
            && root.loopBackend.display_peaks.length > 0
            ? Math.max(...root.loopBackend.display_peaks)
            : 0.0
        onValueChanged: root.updatePeakState()
    }

    ShoopEguiLoopWidget {
        id: canvas
        anchors.fill: parent
        focus: true

        Component.onCompleted: {
            root.canvasReady = true
            root.updateCanvas()
            root.updatePeakState()
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
