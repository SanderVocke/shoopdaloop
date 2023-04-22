import QtQuick 2.15
import QtQuick.Controls 2.15

Item {
    property Registry registry
    property var select_fn: (o) => false
    property bool values_only : false

    // Only found objects
    property list<var> objects : []

    function update() {
        console.log("update")
        if (values_only) {
            objects = registry.select_values(select_fn)
        } else {
            objects = registry.select(select_fn)
        }
    }

    Component.onCompleted: update()
    onRegistryChanged: update()
    onSelect_fnChanged: update()
    Connections {
        target: registry
        function onContentsChanged() { update() }
    }
}