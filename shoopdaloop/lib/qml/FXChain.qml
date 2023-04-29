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

    function actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to) {
        return {
            'schema': 'fx_chain.1',
            'id': obj_id,
            'type': descriptor.type,
            'ports': ports_mapper.instances.map(i => i.actual_session_descriptor(do_save_data_files, data_files_dir, add_tasks_to))
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
        } else {
            throw new Error("Completed an FX chain object but no descriptor")
        }
    }

    function qml_close() {
        reg_entry.close()
        close()
    }

    Mapper {
        id: ports_mapper
        model: descriptor.ports.filter(p => p.schema == 'audioport.1')
        
        AudioPort {
            property var mapped_item
            property int index
            descriptor: mapped_item
            objects_registry: root.objects_registry
            state_registry: root.state_registry
            is_internal: true

            onInitializedChanged: console.log("FX AUDIO PORT INITIALIZED: ", initialized)
            Component.onCompleted: console.log("FX AUDIO PORT COMPLETED. INITIALIZED:", initialized, direction, is_internal, backend)
        }
    }
    // TODO MIDI ports
}