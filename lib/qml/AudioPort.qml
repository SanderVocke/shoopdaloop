import AudioPort
import QtQuick 2.15

import '../../build/types.js' as Types

AudioPort {
    id: port
    property var descriptor : null
    property Registry objects_registry : null
    property Registry state_registry : null
    property bool loaded : initialized

    readonly property string obj_id : descriptor.id

    passthrough_to : lookup_passthrough_to.objects

    function actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to) {
        return {
            'schema': 'audioport.1',
            'id': descriptor.id,
            'name_parts': descriptor.name_parts,
            'direction': descriptor.direction,
            'volume': volume,
            'passthrough_to': descriptor.passthrough_to // TODO test properly
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
        schema: 'audioport.1'
    }
    Component.onCompleted: {
        if(objects_registry) { objects_registry.register(descriptor.id, this) }
    }
    function qml_close() {
        objects_registry.unregister(descriptor.id)
        close()
    }
    property list<string> name_parts : descriptor.name_parts
    name_hint : name_parts.join('')
    direction : descriptor.direction == 'input' ? Types.PortDirection.Input :
                descriptor.direction == 'output' ? Types.PortDirection.Output :
                undefined
    volume : descriptor.volume
}