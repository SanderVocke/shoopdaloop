import QtQuick 6.3
import QtQuick.Controls 6.3

Item {
    property Registry registry
    property list<string> keys : []

    // Only found objects
    property list<var> objects : []

    // This list matches the keys list in shape, with
    // non-found objects set to null.
    property list<var> objects_exact : keys.map(() => null)

    // If true, the objects list will only contain found objects.
    // If false, the shape of the objects list will match the shape
    // of the keys list, with missing items set to null.
    property bool filter_missing: true

    function update() {
        var available = keys.map((key) => (registry && registry.has(key)))
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
    Connections {
        target: registry
        function onItemAdded(key, item) { if (keys.includes(key)) { update() } }
        function onItemRemoved(key) { if (keys.includes(key)) { update() } }
        function onItemModified(key, item) { if (keys.includes(key)) { update() } }
    }
}