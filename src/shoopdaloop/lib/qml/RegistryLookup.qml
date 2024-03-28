import QtQuick 6.6
import QtQuick.Controls 6.6
import ShoopDaLoop.PythonLogger

Item {
    property var registry
    property string key
    property var object : null

    property PythonLogger logger: PythonLogger { name: "Frontend.Qml.RegistryLookup" }

    function update() {
        object = (registry && key.length > 0 && registry.has(key)) ?
            registry.get(key) : null;
        logger.debug(() => `Registry lookup w/ parent ${parent} @ ${registry}:${key}: yielded ${object}`)
    }

    Component.onCompleted: update()
    onRegistryChanged: update()
    onKeyChanged: update()
    Connections {
        target: registry
        function onItemAdded (id, val) { if(id == key) { update() } }
        function onItemModified (id, val) { if(id == key) { update() } }
        function onItemRemoved (id) { if(id == key) { update() } }
    }
}