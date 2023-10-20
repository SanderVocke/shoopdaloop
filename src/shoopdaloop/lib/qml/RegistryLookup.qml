import QtQuick 6.3
import QtQuick.Controls 6.3

Item {
    property var registry
    property string key
    property var object : null

    function update() {
        object = (registry && registry.has(key)) ?
            registry.get(key) : null;
    }

    Component.onCompleted: update()
    onRegistryChanged: update()
    onKeyChanged: update()
    Connections {
        target: registry
        function onItemAdded (id, val) { if(id == key) { object = val; objectChanged() } }
        function onItemModified (id, val) { if(id == key) { object = val; objectChanged() } }
        function onItemRemoved (id) { if(id == key) { object = null; objectChanged() } }
    }
}