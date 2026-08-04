import QtQuick 6.6
import ShoopDaLoop.Rust

Item {
    id: root

    visible: false

    property var loopWidget: null
    property var loopStateSink: null
    property var peakStateSink: null
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

    function updateLoopState() {
        if (loopStateSink) {
            loopStateSink(loopState)
        }
    }

    function updatePeakState() {
        if (peakStateSink) {
            peakStateSink(
                stereo,
                stereo ? peakMeterLeft.value : peakMeterOverall.value,
                stereo ? peakMeterRight.value : peakMeterOverall.value,
                midiActivity
            )
        }
    }

    onLoopStateChanged: updateLoopState()
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

    Component.onCompleted: {
        updateLoopState()
        updatePeakState()
    }
}
