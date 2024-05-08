import QtQuick 6.6
import QtQuick.Controls 6.6

Item {
    property var registry
    property var keys : []

    // Only found objects
    property var objects : []

    // This list matches the keys list in shape, with
    // non-found objects set to null.
    property var objects_exact : keys.map(() => null)

    // If true, the objects list will only contain found objects.
    // If false, the shape of the objects list will match the shape
    // of the keys list, with missing items set to null.
    property bool filter_missing: true

    function update() {
        var available = keys.map((key) => (registry && key.length > 0 && registry.has(key)))
        objects_exact = keys.map((key, idx) => {
            return available[idx] ? registry.get(key) : null;
        })
        var _objects = []
        for(var i=0; i<keys.length; i++) {
            if (available[i]) { _objects.push(objects_exact[i])}
        }
        objects = _objects;
    }

    Component.onCompleted: update()
    onRegistryChanged: update()
    onKeysChanged: update()
    Connections {
        target: registry
        function onItemAdded(key, item) { if (keys.includes(key)) { update() } }
        function onItemRemoved(key) { if (keys.includes(key)) { update() } }
        function onItemModified(key, item) { if (keys.includes(key)) { update() } }
    }
}