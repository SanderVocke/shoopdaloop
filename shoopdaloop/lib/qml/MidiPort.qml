import MidiPort
import QtQuick 2.15

import '../backend/frontend_interface/types.js' as Types

MidiPort {
    id: port
    property var descriptor : null
    property Registry objects_registry : null
    property Registry state_registry : null
    property bool loaded : initialized
    direction: {
        switch(descriptor.direction) {
            case 'input': return Types.PortDirection.Input;
            case 'output': return Types.PortDirection.Output;
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
            'passthrough_to': descriptor.passthrough_to
        }
    }
    function queue_load_tasks(data_files_dir, add_tasks_to) {}

    RegistryLookups {
        id: lookup_passthrough_to
        registry: objects_registry
        keys: descriptor.passthrough_to
    }

    SchemaCheck {
        descriptor: port.descriptor
        schema: 'midiport.1'
    }

    RegisterInRegistry {
        id: reg_entry
        registry: port.objects_registry
        key: port.descriptor.id
        object: port
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