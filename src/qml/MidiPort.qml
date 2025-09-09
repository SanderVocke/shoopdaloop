import ShoopDaLoop.Rust
import QtQuick 6.6

ShoopRustPortGui {
    id: root
    property var descriptor : null
    property bool loaded : initialized
    property var logger : ShoopRustLogger {
        name: "Frontend.Qml.MidiPort"
        instance_identifier: root.obj_id
    }

    RequireBackend {}

    function parse_connectability(conn) {
        var rval = 0
        if (conn.includes('internal')) {
            rval |= ShoopRustConstants.PortConnectability.Internal
        }
        if (conn.includes('external')) {
            rval |= ShoopRustConstants.PortConnectability.External
        }
        return rval
    }

    input_connectability : parse_connectability(descriptor.input_connectability)
    output_connectability : parse_connectability(descriptor.output_connectability)

    readonly property string obj_id : descriptor.id

    internal_port_connections : lookup_internal_port_connections.objects

    function actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to) {
        return {
            'schema': 'midiport.1',
            'id': descriptor.id,
            'name_parts': descriptor.name_parts,
            'type': descriptor.type,
            'input_connectability': descriptor.input_connectability,
            'output_connectability': descriptor.output_connectability,
            'muted': muted,
            'passthrough_muted': passthrough_muted,
            'internal_port_connections': descriptor.internal_port_connections,
            'external_port_connections': get_connected_external_ports(),
            'min_n_ringbuffer_samples': descriptor.min_n_ringbuffer_samples
        }
    }
    function queue_load_tasks(data_files_dir, from_sample_rate, to_sample_rate, add_tasks_to) {}

    Component.onCompleted: push_all()
    Component.onDestruction: root.logger.debug("destruct")

    function push_all() {
        push_muted(descriptor.muted)
        push_passthrough_muted(descriptor.passthrough_muted)
        try_make_connections(descriptor.external_port_connections)
    }

    RegistryLookups {
        id: lookup_internal_port_connections
        registry: AppRegistries.objects_registry
        keys: descriptor.internal_port_connections
    }

    readonly property string object_schema : 'midiport.1'
    SchemaCheck {
        descriptor: root.descriptor
        schema: root.object_schema
    }

    RegisterInRegistry {
        id: reg_entry
        registry: AppRegistries.objects_registry
        key: root.descriptor.id
        object: root
    }

    function unload() {
        root.logger.debug("unloading: " + root.name_hint)
        reg_entry.close()
        deinit()
    }

    property var name_parts : descriptor.name_parts
    name_hint : name_parts.join('')
    min_n_ringbuffer_samples: descriptor.min_n_ringbuffer_samples
    maybe_fx_chain: null
    fx_chain_port_idx: 0
    is_midi: true
}