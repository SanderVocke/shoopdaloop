import ShoopDaLoop.Rust
import ShoopDaLoop.PythonLogger
import QtQuick 6.6

import ShoopConstants

ShoopRustPortGui {
    id: root
    property var descriptor : null
    property bool loaded : initialized
    property var logger : PythonLogger {
        name: "Frontend.Qml.AudioPort"
        instanceIdentifier: root.obj_id
    }

    RequireBackend {}

    function parse_connectability(conn) {
        var rval = 0
        if (conn.includes('internal')) {
            rval |= ShoopConstants.PortConnectability.Internal
        }
        if (conn.includes('external')) {
            rval |= ShoopConstants.PortConnectability.External
        }
        return rval
    }

    input_connectability : parse_connectability(descriptor.input_connectability)
    output_connectability : parse_connectability(descriptor.output_connectability)

    readonly property string obj_id : descriptor.id

    internal_port_connections : lookup_internal_port_connections.objects

    function actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to) {
        return {
            'schema': 'audioport.1',
            'id': descriptor.id,
            'name_parts': descriptor.name_parts,
            'type': descriptor.type,
            'input_connectability': descriptor.input_connectability,
            'output_connectability': descriptor.output_connectability,
            'gain': audio_gain,
            'muted': muted,
            'passthrough_muted': passthrough_muted,
            'internal_port_connections': descriptor.internal_port_connections,
            'external_port_connections': get_connected_external_ports(),
            'min_n_ringbuffer_samples': descriptor.min_n_ringbuffer_samples
        }
    }
    function queue_load_tasks(data_files_dir, from_sample_rate, to_sample_rate, add_tasks_to) {}

    Component.onCompleted: {
        root.logger.trace("onCompleted for AudioPort. descriptor: " + JSON.stringify(descriptor, null, 2))
        push_all()
    }
    
    function push_all() {
        push_muted(descriptor.muted)
        push_passthrough_muted(descriptor.passthrough_muted)
        push_audio_gain(descriptor.gain)
        try_make_connections(descriptor.external_port_connections)
    }

    RegistryLookups {
        id: lookup_internal_port_connections
        registry: registries.objects_registry
        keys: descriptor.internal_port_connections
    }

    readonly property string object_schema : 'audioport.1'
    SchemaCheck {
        descriptor: root.descriptor
        schema: root.object_schema
    }

    RegisterInRegistry {
        id: reg_entry
        registry: registries.objects_registry
        key: root.descriptor.id
        object: root
    }

    function unload() {
        reg_entry.close()
        close()
        destroy()
    }

    property var name_parts : descriptor.name_parts
    name_hint : name_parts.join('')
    min_n_ringbuffer_samples: descriptor.min_n_ringbuffer_samples
    maybe_fx_chain: null
    fx_chain_port_idx: 0
    is_midi: false

    onAudio_gain_changed: { root.logger.debug("gain -> " + root.audio_gain) }
    onMutedChanged: { root.logger.debug("muted -> " + root.muted) }
    onPassthrough_mutedChanged: { root.logger.debug("passthrough muted -> " + root.passthrough_muted) }
}