import QtQuick 6.6
import ShoopDaLoop.Rust

Item {
    id: root

    visible: false

    property var trackWidget: null
    property var stateSink: null
    readonly property var control: trackWidget ? trackWidget.control_widget : null
    readonly property var audioOutputs: control && Array.isArray(control.audio_out_ports)
        ? control.audio_out_ports : []
    readonly property var audioInputs: control && Array.isArray(control.audio_in_ports)
        ? control.audio_in_ports : []
    readonly property var midiOutputs: control && Array.isArray(control.midi_out_ports)
        ? control.midi_out_ports : []
    readonly property var midiInputs: control && Array.isArray(control.midi_in_ports)
        ? control.midi_in_ports : []

    readonly property var state: ({
        name: trackWidget ? trackWidget.name : "Track",
        hasOutput: audioOutputs.length + midiOutputs.length > 0,
        hasOutputAudio: audioOutputs.length > 0,
        outputStereo: control ? control.out_is_stereo : false,
        outputGainDb: control ? control.gain_dB : 0.0,
        outputBalance: control ? control.output_balance : 0.0,
        outputMuted: control ? control.mute : false,
        outputPeakLeftDb: control && control.out_is_stereo ? outputPeakLeft.value : outputPeakOverall.value,
        outputPeakRightDb: control && control.out_is_stereo ? outputPeakRight.value : outputPeakOverall.value,
        outputMidiActivity: control
            ? control.n_midi_notes_active_out > 0 || control.n_midi_events_out > 0
            : false,
        hasInput: audioInputs.length + midiInputs.length > 0,
        hasInputAudio: audioInputs.length > 0,
        inputStereo: control ? control.in_is_stereo : false,
        inputGainDb: control ? control.input_gain_dB : 0.0,
        inputBalance: control ? control.input_balance : 0.0,
        inputMonitoring: control ? control.monitor : false,
        inputPeakLeftDb: control && control.in_is_stereo ? inputPeakLeft.value : inputPeakOverall.value,
        inputPeakRightDb: control && control.in_is_stereo ? inputPeakRight.value : inputPeakOverall.value,
        inputMidiActivity: control
            ? control.n_midi_notes_active_in > 0 || control.n_midi_events_in > 0
            : false
    })

    function updateState() {
        if (stateSink) stateSink(state)
    }

    onStateChanged: updateState()

    AudioLevelMeterModel {
        id: outputPeakLeft
        max_dt: 0.1
        input: root.audioOutputs.length > 0 ? root.audioOutputs[0].audio_input_peak : 0.0
        onValueChanged: root.updateState()
    }
    AudioLevelMeterModel {
        id: outputPeakRight
        max_dt: 0.1
        input: root.audioOutputs.length > 1 ? root.audioOutputs[1].audio_input_peak : 0.0
        onValueChanged: root.updateState()
    }
    AudioLevelMeterModel {
        id: outputPeakOverall
        max_dt: 0.1
        input: root.audioOutputs.length > 0
            ? Math.max(...root.audioOutputs.map(port => port.audio_input_peak)) : 0.0
        onValueChanged: root.updateState()
    }
    AudioLevelMeterModel {
        id: inputPeakLeft
        max_dt: 0.1
        input: root.audioInputs.length > 0 ? root.audioInputs[0].audio_input_peak : 0.0
        onValueChanged: root.updateState()
    }
    AudioLevelMeterModel {
        id: inputPeakRight
        max_dt: 0.1
        input: root.audioInputs.length > 1 ? root.audioInputs[1].audio_input_peak : 0.0
        onValueChanged: root.updateState()
    }
    AudioLevelMeterModel {
        id: inputPeakOverall
        max_dt: 0.1
        input: root.audioInputs.length > 0
            ? Math.max(...root.audioInputs.map(port => port.audio_input_peak)) : 0.0
        onValueChanged: root.updateState()
    }

    Component.onCompleted: updateState()
}
