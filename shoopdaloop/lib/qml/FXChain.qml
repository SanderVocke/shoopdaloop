import FXChain
import QtQuick 2.15

import '../backend/frontend_interface/types.js' as Types

FXChain {
    id: root
    property bool loaded : initialized

    property var descriptor : null
    property Registry objects_registry: null
    property Registry state_registry: null

    readonly property string obj_id : descriptor.id
    title: descriptor.title

    function actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to) {
        return {
            'schema': 'fx_chain.1',
            'id': obj_id,
            'title': title,
            'type': descriptor.type,
            'ports': all_ports().map(i => i.actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to)),
            'internal_state': get_internal_state()
        }
    }

    SchemaCheck {
        descriptor: root.descriptor
        schema: 'fx_chain.1'
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
            }

            if ('internal_state' in descriptor) {
                var restore = function() {
                    root.restore_internal_state(descriptor.internal_state)
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