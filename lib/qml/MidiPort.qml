import MidiPort
import QtQuick 2.15

MidiPort {
    id: port
    property var descriptor : null
    property Registry objects_registry : null
    property Registry state_registry : null

    SchemaCheck {
        descriptor: port.descriptor
        schema: 'midiport.1'
    }
    Component.onCompleted: if(objects_registry) { objects_registry.register(descriptor.id, this) }
    Component.onDestruction: close()

    direction : descriptor.direction
    name_hint : descriptor.name
}