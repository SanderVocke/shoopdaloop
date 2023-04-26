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

    onChain_typeChanged: console.log("chain type", chain_type)
    onBackendChanged: console.log("backend", backend)
    onInitializedChanged: console.log("FX chain initialized")

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
        console.log("FXChain.qml: completed")
        if (descriptor) {
            switch(descriptor.type) {
                case "carla_rack": chain_type = Types.FXChainType.Carla_Rack; break;
                case "carla_patchbay": chain_type = Types.FXChainType.Carla_Patchbay; break;
                case "carla_patchbay_16x": chain_type = Types.FXChainType.Carla_Patchbay_16x; break;
            }
            console.log("FXChain.qml: type", descriptor.type, chain_type)
            set_ui_visible(true)
        } else {
            throw new Error("Completed an FX chain object but no descriptor")
        }
    }

    function qml_close() {
        reg_entry.close()
        close()
    }
}