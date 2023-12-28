import ShoopDaLoop.PythonMidiPort
import QtQuick 6.3

import ShoopConstants

PythonMidiPort {
    id: root
    property var descriptor : null
    property bool loaded : initialized
    direction: {
        switch(descriptor.direction) {
            case 'input': return ShoopConstants.PortDirection.Input;
            case 'output': return ShoopConstants.PortDirection.Output;
        }
        throw new Error("No direction for midi port")
    }

    readonly property string obj_id : descriptor.id

    passthrough_to : lookup_passthrough_to.objects

    function actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to) {
        return {
            'schema': 'midiport.1',
            'id': descriptor.id,
            'name_parts': descriptor.name_parts,
            'direction': descriptor.direction,
            'muted': descriptor.muted,
            'passthrough_muted': descriptor.muted,
            'passthrough_to': descriptor.passthrough_to,
            'external_port_connections': get_connected_external_ports()
        }
    }
    function queue_load_tasks(data_files_dir, from_sample_rate, to_sample_rate, add_tasks_to) {}

    Component.onCompleted: try_make_connections(descriptor.external_port_connections)
    onInitializedChanged: try_make_connections(descriptor.external_port_connections)

    RegistryLookups {
        id: lookup_passthrough_to
        registry: registries.objects_registry
        keys: descriptor.passthrough_to
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