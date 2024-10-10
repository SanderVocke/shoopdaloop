import QtQuick 6.6
import QtQuick.Controls 6.6

Item {
    property var registry
    property var select_fn: (o) => false
    property bool values_only : false

    // Only found objects
    property var objects : []

    function update() {
        if (registry) {
            if (values_only) {
                objects = registry.select_values(select_fn)
            } else {
                objects = registry.select(select_fn)
            }
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