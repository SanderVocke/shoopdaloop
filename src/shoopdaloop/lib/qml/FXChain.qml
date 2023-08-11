import ShoopDaLoop.PythonFXChain
import QtQuick 6.3

import '../generated/types.js' as Types

PythonFXChain {
    id: root
    property bool loaded : initialized

    property var descriptor : null
    property Registry objects_registry: null
    property Registry state_registry: null

    readonly property string obj_id : descriptor.id
    title: descriptor.title

    function actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to) {
        if (!descriptor) { return null; }
        return {
            'schema': 'fx_chain.1',
            'id': obj_id,
            'title': title,
            'type': descriptor.type,
            'ports': all_ports().map(i => i.actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to)),
            'internal_state': get_state_str()
        }
    }

    readonly property string object_schema : 'fx_chain.1'
    SchemaCheck {
        descriptor: root.descriptor
        schema: root.object_schema
    }

    RegisterInRegistry {
        id: reg_entry
        registry: root.objects_registry
        key: root.descriptor.id
        object: root
    }

    Component.onCompleted: {
        if (descriptor) {
            switch(descriptor.type) {
                case "carla_rack": chain_type = Types.FXChainType.Carla_Rack; break;
                case "carla_patchbay": chain_type = Types.FXChainType.Carla_Patchbay; break;
                case "carla_patchbay_16x": chain_type = Types.FXChainType.Carla_Patchbay_16x; break;
                case "test2x2x1": chain_type = Types.FXChainType.Test2x2x1; break;
            }

            if ('state_str' in descriptor) {
                var restore = function() {
                    root.restore_state(descriptor.state_str)
                }
                if (initialized) { restore() }
                else { root.initializedChanged.connect(restore) }
            }

        } else {
            throw new Error("Completed an FX chain object but no descriptor")
        }
    }

    function qml_close() {
        reg_entry.close()
        close()
    }

    function all_ports() {
        return [...audio_ports_mapper.instances, ...midi_ports_mapper.instances]
    }

    Mapper {
        id: audio_ports_mapper
        model: descriptor.ports.filter(p => p.schema == 'audioport.1')
        
        AudioPort {
            property var mapped_item
            property int index
            descriptor: mapped_item
            objects_registry: root.objects_registry
            state_registry: root.state_registry
            is_internal: true
        }
    }
    Mapper {
        id: midi_ports_mapper
        model: descriptor.ports.filter(p => p.schema == 'midiport.1')
        
        MidiPort {
            property var mapped_item
            property int index
            descriptor: mapped_item
            objects_registry: root.objects_registry
            state_registry: root.state_registry
            is_internal: true
        }
    }
}