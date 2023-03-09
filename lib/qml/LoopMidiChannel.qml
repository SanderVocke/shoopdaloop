import LoopMidiChannel
import QtQuick 2.15

import '../../build/types.js' as Types
import '../session_schemas/conversions.js' as Conversions

LoopMidiChannel {
    id: chan

    property var descriptor : null
    property Registry objects_registry : null
    property Registry state_registry : null

    readonly property string obj_id : descriptor.id

    SchemaCheck {
        descriptor: chan.descriptor
        schema: 'channel.1'
    }

    function actual_session_descriptor(do_save_data_files, data_files_dir) {
        var rval = {
            'schema': 'channel.1',
            'id': obj_id,
            'mode': initialized ? Conversions.stringify_channel_mode(mode) : descriptor.mode,
            'type': 'midi',
            'data_length': data_length,
            'connected_port_ids': initialized ? connected_ports.map((c) => c.obj_id) : descriptor.connected_port_ids
        }
        
        if (do_save_data_files && data_length > 0) {
            var filename = obj_id + '.mid'
            var full_filename = data_files_dir + '/' + filename;
            save_data(full_filename)
            rval['data_file'] = filename
        }
        return rval
    }

    property int initial_mode : Conversions.parse_channel_mode(descriptor.mode)
    onInitial_modeChanged: set_mode(initial_mode)
    ports: lookup_connected_ports.objects

    RegistryLookups {
        id: lookup_connected_ports
        registry: objects_registry
        keys: descriptor.connected_port_ids
    }

    Component.onCompleted: {
        set_mode(initial_mode)
        if(objects_registry) { objects_registry.register(descriptor.id, this) }
    }
    function qml_close() {
        objects_registry.unregister(descriptor.id)
        close()
    }
}