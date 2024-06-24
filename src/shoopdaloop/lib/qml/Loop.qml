import ShoopDaLoop.PythonLoop
import ShoopDaLoop.PythonLogger
import QtQuick 6.6

import ShoopConstants
import '../mode_helpers.js' as ModeHelpers

PythonLoop {
    property bool loaded : initialized

    readonly property PythonLogger logger: PythonLogger {
        name: "Frontend.Qml.Loop"
        instanceIdentifier: obj_id
    }
    onObj_idChanged: instanceIdentifier = obj_id

    property var maybe_fx_chain: null
    property var loop_widget : null

    // Gives a nice full progress bar when recording
    readonly property int display_position: mode == ShoopConstants.LoopMode.Recording ? length : position

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
                    if (c.mode == ShoopConstants.ChannelMode.Wet && maybe_fx_chain) {
                        if (!fx_chain_desc_id) {
                            root.logger.debug(`Caching FX chain state for wet channel ${c.obj_id}`)
                            fx_chain_desc_id = registries.fx_chain_states_registry.generate_id('fx_chain_state')
                            var fx_chain_desc = maybe_fx_chain.actual_session_descriptor()
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
        get_audio_channels().forEach(c => c.qml_close())
        get_midi_channels().forEach(c => c.qml_close())
        close()
    }

    property var initial_descriptor: null
    property string obj_id: initial_descriptor ? initial_descriptor.id : null
    RegistryLookups {
        keys: (root.initial_descriptor && root.initial_descriptor.channels) ? root.initial_descriptor.channels.map(c => c.id) : []
        registry: registries.objects_registry
        id: lookup_channels
    }
    property alias channels: lookup_channels.objects
}