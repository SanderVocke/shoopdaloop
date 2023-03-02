import AudioPort
import QtQuick 2.15

AudioPort {
    id: port
    property var descriptor : null
    property Registry objects_registry : null
    property Registry state_registry : null
    property bool loaded : initialized

    onLoadedChanged: if(loaded) { console.log("LOADED: AudioPort") }

    SchemaCheck {
        descriptor: port.descriptor
        schema: 'audioport.1'
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
    volume : descriptor.volume
}