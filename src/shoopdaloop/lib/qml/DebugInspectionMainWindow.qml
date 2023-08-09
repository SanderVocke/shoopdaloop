import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3

ApplicationWindow {
    
    id: root
    title: "Debug Inspections"

    property var objects_registry

    width: 800
    height: 250
    minimumWidth: 200
    minimumHeight: 50
    
    Material.theme: Material.Dark

    property var object_ids: objects_registry ? objects_registry.keys() : []
    Connections {
        target: objects_registry
        function onContentsChanged() {
            object_ids = objects_registry.keys()
        }
    }

    Row {
        ComboBox {
            id: filter
            model: ["Any", "Port", "Loop", "Channel", "FX"]
        }
        ComboBox {
            model: {
                switch(filter.currentValue) {
                    case "Any":
                        return root.object_ids
                    case "Port":
                        return root.object_ids.filter(id => objects_registry.get(id) instanceof Port)
                    case "Loop":
                        return root.object_ids.filter(id => objects_registry.get(id) instanceof Loop)
                    case "Channel":
                        return root.object_ids.filter(id => objects_registry.get(id) instanceof Channel)
                    case "FX":
                        return root.object_ids.filter(id => objects_registry.get(id) instanceof FX)
                }
            }
            width: 400
        }
        Button {
            text: "Inspect"
        }
    }
}