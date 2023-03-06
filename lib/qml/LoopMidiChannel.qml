import LoopMidiChannel
import QtQuick 2.15

import '../../build/types.js' as Types

LoopMidiChannel {
    id: chan

    property var descriptor : null
    property Registry objects_registry : null
    property Registry state_registry : null

    SchemaCheck {
        descriptor: chan.descriptor
        schema: 'channel.1'
    }

    property int initial_mode : {
        switch(descriptor.mode) {
            case "direct" : return Types.ChannelMode.Direct;
            case "disabled" : return Types.ChannelMode.Disabled;
            case "dry" : return Types.ChannelMode.Dry;
            case "wet" : return Types.ChannelMode.Wet;
        }
    }
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