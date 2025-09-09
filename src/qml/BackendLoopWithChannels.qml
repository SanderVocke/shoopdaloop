import QtQuick 6.6
import QtQuick.Controls 6.6
import ShoopDaLoop.Rust
import 'js/mode_helpers.js' as ModeHelpers

// Wrap a Loop that may be dynamically loaded in a just-in-time way.
Loop {
    id: root
    objectName: "Qml.BackendLoopWithChannels"

    readonly property var loop : root
    readonly property var maybe_backend_loop: root

    readonly property ShoopRustLogger logger : ShoopRustLogger { name:"Frontend.Qml.BackendLoopWithChannels" }

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
            total += channel.midi_n_events_triggered;
        }
        return total
    }

    property bool dummy_trigger_recalc_display_midi_notes_active: false
    readonly property int display_midi_notes_active: {
        // To manually trigger
        dummy_trigger_recalc_display_midi_notes_active;

        var total = 0;
        for (var channel of midi_channels) {
            total += channel.midi_n_notes_active;
        }
        return total
    }

    Component.onCompleted: {
        queue_set_length(initial_descriptor.length)
    }

    function is_all_empty() {
        return length == 0 && audio_channels.every(c => c.is_empty()) && midi_channels.every(c => c.is_empty())
    }

    function unload() {
        audio_channels.forEach(c => c.unload())
        midi_channels.forEach(c => c.unload())
        close()
        destroy()
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
            id: self

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
    property var channels: [...audio_channels, ...midi_channels]

    function get_channels() {
        return [...audio_channels, ...midi_channels]
    }

    // TODO: kind of a bad place to put this.
    // When the loop starts recording, ensure all channels have their
    // "recording started at" updated and also our FX chain state is
    // cached for wet channels.
    property var prev_mode: mode
    onModeChanged: {
        if (prev_mode != mode) {
            var now = (new Date()).toISOString()
            var fx_chain_desc_id = null

            var channel_fn = (c => {
                if (ModeHelpers.is_recording_mode_for(mode, c.mode)) {
                    c.recording_started_at = now
                    if (c.mode == ShoopRustConstants.ChannelMode.Wet && maybe_fx_chain) {
                        if (!fx_chain_desc_id) {
                            root.logger.debug(`Caching FX chain state for wet channel ${c.obj_id}`)
                            fx_chain_desc_id = AppRegistries.fx_chain_states_registry.generate_id('fx_chain_state')
                            var fx_chain_desc = maybe_fx_chain.actual_session_descriptor()
                            delete fx_chain_desc.ports // Port descriptions not needed for state caching, this is track-dependent
                            fx_chain_desc.title = ""   // No title indicates elsewhere that this is not a snapshot that the user can directly see in the list.
                            fx_chain_desc.id = fx_chain_desc_id
                            AppRegistries.fx_chain_states_registry.register (fx_chain_desc_id, fx_chain_desc)
                        }
                        c.recording_fx_chain_state_id = fx_chain_desc_id
                    }
                }
            })

            audio_channels.forEach(channel_fn)
            midi_channels.forEach(channel_fn)
        }
        prev_mode = mode
    }
}