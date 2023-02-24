import LoopAudioChannel
import QtQuick 2.15

import '../../build/types.js' as Types

LoopAudioChannel {
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
    ports: descriptor.connected_port_ids
                    .filter(id => objects_registry.has(id))
                    .map(id => objects_registry.get(id))
    
    Component.onCompleted: {
        set_mode(initial_mode)
        if(objects_registry) { objects_registry.register(descriptor.id, this) }
        update_ports()
    }
    Connections {
        target: objects_registry
        // React to ports being instantiated later than channels
        function onItemAdded(id, obj) {
            if(descriptor.connected_port_ids.includes(id) && !ports.includes(obj)) {
                ports.push(obj)
            }
        }
    }
}