import ShoopDaLoop.Rust
import QtQuick 6.6

ShoopRustFXChainGui {
    id: root
    property bool loaded : (initialized &&
                            audio_input_ports_mapper.loaded &&
                            audio_output_ports_mapper.loaded &&
                            midi_input_ports_mapper.loaded)

    RequireBackend {}

    readonly property var logger : ShoopRustLogger { name: "Frontend.Qml.FXChain" }

    property var descriptor : null

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
        registry: AppRegistries.objects_registry
        key: root.descriptor.id
        object: root
    }

    Component.onCompleted: {
        if (descriptor) {
            switch(descriptor.type) {
                case "carla_rack": chain_type = ShoopRustConstants.FXChainType.CarlaRack; break;
                case "carla_patchbay": chain_type = ShoopRustConstants.FXChainType.CarlaPatchbay; break;
                case "carla_patchbay_16x": chain_type = ShoopRustConstants.FXChainType.CarlaPatchbay16x; break;
                case "test2x2x1": chain_type = ShoopRustConstants.FXChainType.Test2x2x1; break;
            }

            if ('internal_state' in descriptor) {
                var restore = function(state_str = descriptor.internal_state) {
                    root.restore_state(state_str)
                }
                if (initialized) { restore() }
                else { root.initialized_changed.connect(function() { restore() }) }
            }

        } else {
            throw new Error("Completed an FX chain object but no descriptor")
        }
    }

    function unload() {
        reg_entry.close()
        close()
    }

    function all_ports() {
        return [...audio_input_ports_mapper.unsorted_instances.map(l => l.item),
                ...audio_output_ports_mapper.unsorted_instances.map(l => l.item),
                ...midi_input_ports_mapper.unsorted_instances.map(l => l.item)]
    }

    Mapper {
        id: audio_input_ports_mapper
        model: descriptor.ports.filter(p => p.schema == 'audioport.1' && p.output_connectability.length == 0)

        property bool loaded : {
            if (unsorted_instances.length != model.length) { return false; }
            var result = true;
            for (var i=0; i<unsorted_instances.length; i++) {
                result = result && unsorted_instances[i].loaded;
            }
            return result
        }             
        
        Loader {
            active: root.initialized
            property bool loaded: active && item.loaded
            property var mapped_item
            property int index

            sourceComponent: AudioPort {
                descriptor: mapped_item
                is_internal: true
                backend: root.backend
                maybe_fx_chain: root
                fx_chain_port_idx: index
            }
        }
    }
    Mapper {
        id: audio_output_ports_mapper
        model: descriptor.ports.filter(p => p.schema == 'audioport.1' && p.input_connectability.length == 0)

        property bool loaded : {
            if (unsorted_instances.length != model.length) { return false; }
            var result = true;
            for (var i=0; i<unsorted_instances.length; i++) {
                result = result && unsorted_instances[i].loaded;
            }
            return result
        }             
        
        Loader {
            active: root.initialized
            property bool loaded: active && item.loaded
            property var mapped_item
            property int index

            sourceComponent: AudioPort {
                descriptor: mapped_item
                is_internal: true
                backend: root.backend
                maybe_fx_chain: root
                fx_chain_port_idx: index
            }
        }
    }
    Mapper {
        id: midi_input_ports_mapper
        model: descriptor.ports.filter(p => p.schema == 'midiport.1')

        property bool loaded : {
            if (unsorted_instances.length != model.length) { return false; }
            var result = true;
            for (var i=0; i<unsorted_instances.length; i++) {
                result = result && unsorted_instances[i].loaded;
            }
            return result
        }  
        
        Loader {
            active: root.initialized
            property bool loaded: active && item.loaded
            property var mapped_item
            property int index

            sourceComponent: MidiPort {
                descriptor: mapped_item
                is_internal: true
                backend: root.backend
                maybe_fx_chain: root
                fx_chain_port_idx: index
            }
        }
    }
}