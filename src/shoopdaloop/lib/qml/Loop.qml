import ShoopDaLoop.PythonLoop
import ShoopDaLoop.PythonLogger
import QtQuick 6.3

import '../generated/types.js' as Types

PythonLoop {
    property bool loaded : initialized

    readonly property PythonLogger logger: PythonLogger { name: "Frontend.Qml.Loop" }

    // Gives a nice full progress bar when recording
    readonly property int display_position: mode == Types.LoopMode.Recording ? length : position

    // TODO: kind of a bad place to put this.
    // When the loop starts recording, ensure all channels have their
    // "recording started at" updated and also our FX chain state is
    // cached for wet channels.
    property var prev_mode: mode
    onModeChanged: {
        if (prev_mode != mode) {
            var now = (new Date()).toISOString()
            var fx_chain_desc_id = null
            channels.forEach(c => {
                if (ModeHelpers.is_recording_mode_for(mode, c.mode)) {
                    c.recording_started_at = now
                    if (c.mode == Types.ChannelMode.Wet && track_widget.maybe_fx_chain) {
                        if (!fx_chain_desc_id) {
                            fx_chain_desc_id = registries.fx_chain_states_registry.generate_id('fx_chain_state')
                            var fx_chain_desc = track_widget.maybe_fx_chain.actual_session_descriptor()
                            delete fx_chain_desc.ports // Port descriptions not needed for state caching, this is track-dependent
                            fx_chain_desc.title = ""   // No title indicates elsewhere that this is not a snapshot that the user can directly see in the list.
                            fx_chain_desc.id = fx_chain_desc_id
                            registries.fx_chain_states_registry.register (fx_chain_desc_id, fx_chain_desc)
                        }
                        c.recording_fx_chain_state_id = fx_chain_desc_id
                    }
                }
            })
        }
        prev_mode = mode
    }

    function qml_close() {
        for(var i=0; i<audio_channels.model; i++) {
            audio_channels.itemAt(i).qml_close();
        }
        for(var i=0; i<midi_channels.model; i++) {
            midi_channels.itemAt(i).qml_close();
        }
        close()
    }

    property var initial_descriptor: null
    RegistryLookups {
        keys: (root.initial_descriptor && root.initial_descriptor.channels) ? root.initial_descriptor.channels.map(c => c.id) : []
        registry: registries.objects_registry
        id: lookup_channels
    }
    property alias channels: lookup_channels.objects
    property var audio_channels: channels.filter(c => c && c.descriptor.type == 'audio')
    property var midi_channels: channels.filter(c => c && c.descriptor.type == 'midi')
}