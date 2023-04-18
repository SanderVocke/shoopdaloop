import QtQuick 2.15
import QtQuick.Controls 2.15

Item {
    property Registry registry
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