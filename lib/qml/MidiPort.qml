import MidiPort
import QtQuick 2.15

MidiPort {
    id: port
    property var descriptor : null
    property Registry objects_registry : null
    property Registry state_registry : null
    property bool loaded : initialized

    passthrough_to : lookup_passthrough_to.objects

    RegistryLookups {
        id: lookup_passthrough_to
        registry: objects_registry
        keys: descriptor.passthrough_to
    }

    SchemaCheck {
        descriptor: port.descriptor
        schema: 'midiport.1'
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
    direction : descriptor.direction
}