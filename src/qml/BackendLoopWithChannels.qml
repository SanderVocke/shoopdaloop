import QtQuick 6.6
import QtQuick.Controls 6.6
import ShoopDaLoop.PythonLogger

import ShoopConstants

// Wrap a Loop that may be dynamically loaded in a just-in-time way.
Loop {
    id: root
    objectName: "Qml.BackendLoopWithChannels"

    readonly property var loop : root
    readonly property var maybe_backend_loop: root

    readonly property PythonLogger logger : PythonLogger { name:"Frontend.Qml.BackendLoopWithChannels" }

    readonly property var audio_channel_descriptors: (initial_descriptor && initial_descriptor.channels) ? initial_descriptor.channels.filter(c => c.type == 'audio') : []
    readonly property var midi_channel_descriptors: (initial_descriptor && initial_descriptor.channels) ? initial_descriptor.channels.filter(c => c.type == 'midi') : []

    property bool dummy_trigger_recalc_display_peaks: false
    readonly property var display_peaks: {
        // To manually trigger
        dummy_trigger_recalc_display_peaks;

        return audio_channels.map(c => c.audio_output_peak)
    }

    property bool dummy_trigger_recalc_display_midi_events_triggered: false
    readonly property int display_midi_events_triggered: {
        // To manually trigger
        dummy_trigger_recalc_display_midi_events_triggered;

        var total = 0;
        for (var channel of midi_channels) {
            total += channel.n_events_triggered;
        }
        return total
    }

    property bool dummy_trigger_recalc_display_midi_notes_active: false
    readonly property int display_midi_notes_active: {
        // To manually trigger
        dummy_trigger_recalc_display_midi_notes_active;

        var total = 0;
        for (var channel of midi_channels) {
            total += channel.n_notes_active;
        }
        return total
    }

    Component.onCompleted: {
        queue_set_length(initial_descriptor.length)
    }
    Component.onDestruction: {
        unload()
    }

    function is_all_empty() {
        return length == 0 && audio_channels.every(c => c.is_empty()) && midi_channels.every(c => c.is_empty())
    }

    Mapper {
        id: audio_channels_mapper
        model: root.audio_channel_descriptors

        LoopAudioChannel {
            property var mapped_item
            property int index
            channel_loop: root.loop
            descriptor: mapped_item
            backend : root.backend

            onAudio_output_peak_changed: { root.dummy_trigger_recalc_display_peaks = !root.dummy_trigger_recalc_display_peaks }
        }
    }
    Mapper {
        id: midi_channels_mapper
        model : root.midi_channel_descriptors

        LoopMidiChannel {
            property var mapped_item
            property int index
            channel_loop: root.loop
            descriptor: mapped_item
            backend : root.backend
        }
    }

    property alias audio_channels : audio_channels_mapper.sorted_instances
    property alias midi_channels: midi_channels_mapper.sorted_instances
}