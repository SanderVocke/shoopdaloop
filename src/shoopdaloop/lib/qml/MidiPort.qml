import ShoopDaLoop.PythonMidiPort
import QtQuick 6.3

import ShoopConstants

PythonMidiPort {
    id: root
    property var descriptor : null
    property bool loaded : initialized
    direction: {
        switch(descriptor.driver_direction) {
            case 'input': return ShoopConstants.PortDirection.Input;
            case 'output': return ShoopConstants.PortDirection.Output;
        }
        throw new Error("No direction for midi port: " + JSON.stringify(descriptor, null, 2))
    }

    readonly property string obj_id : descriptor.id

    internal_port_connections : lookup_internal_port_connections.objects

    function actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to) {
        return {
            'schema': 'midiport.1',
            'id': descriptor.id,
            'name_parts': descriptor.name_parts,
            'type': descriptor.type,
            'driver_direction': descriptor.driver_direction,
            'muted': descriptor.muted,
            'passthrough_muted': descriptor.muted,
            'internal_port_connections': descriptor.internal_port_connections,
            'external_port_connections': get_connected_external_ports()
        }
    }
    function queue_load_tasks(data_files_dir, from_sample_rate, to_sample_rate, add_tasks_to) {}

    Component.onCompleted: try_make_connections(descriptor.external_port_connections)
    onInitializedChanged: try_make_connections(descriptor.external_port_connections)

    RegistryLookups {
        id: lookup_internal_port_connections
        registry: registries.objects_registry
        keys: descriptor.internal_port_connections
    }

    readonly property string object_schema : 'midiport.1'
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

    function qml_close() {
        reg_entry.close()
        close()
    }

    property list<string> name_parts : descriptor.name_parts
    name_hint : name_parts.join('')
    muted: descriptor.muted
    passthrough_muted: descriptor.passthrough_muted
}