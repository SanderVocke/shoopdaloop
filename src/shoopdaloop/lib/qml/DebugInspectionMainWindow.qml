import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6

ApplicationWindow {
    
    id: root
    title: "Debug Inspections"

    width: 700
    height: 250
    minimumWidth: 700
    maximumWidth: 700
    minimumHeight: 250
    maximumHeight: 250
    
    Material.theme: Material.Dark

    property var object_ids: registries.objects_registry.keys()
    Connections {
        target: registries.objects_registry ? registries.objects_registry : null
        function onContentsChanged() {
            object_ids = registries.objects_registry.keys()
        }
    }

    readonly property var window_factory : Qt.createComponent("DebugInspectionItemWindow.qml")
    function spawn_window(object) {
        if (window_factory.status == Component.Error) {
            throw new Error("DebugInspectionMainWindow: Failed to load window factory: " + window_factory.errorString())
        } else if (window_factory.status != Component.Ready) {
            throw new Error("DebugInspectionMainWindow: Factory not ready")
        } else {
            var window = window_factory.createObject(root, {
                object: object,
                visible: true
            })
            window.spawn_window.connect(root.spawn_window)
        }
    }

    Row {
        spacing: 5

        ComboBox {
            id: filter
            model: ["Any", "Port", "Loop", "Channel", "FX"]
        }
        ComboBox {
            id: object_select
            anchors.verticalCenter: filter.verticalCenter
            model: {
                switch(filter.currentValue) {
                    case "Any":
                        return root.object_ids
                    case "Port":
                        return root.object_ids.filter(id => registries.objects_registry.get(id).object_schema.match(/(?:audio|midi)port.[0-9]+/))
                    case "Loop":
                        return root.object_ids.filter(id => registries.objects_registry.get(id).object_schema.match(/loop.[0-9]+/))
                    case "Channel":
                        return root.object_ids.filter(id => registries.objects_registry.get(id).object_schema.match(/channel.[0-9]+/))
                    case "FX":
                        return root.object_ids.filter(id => registries.objects_registry.get(id).object_schema.match(/fx_chain.[0-9]+/))
                }
            }
            width: 400
        }
        Button {
            anchors.verticalCenter: filter.verticalCenter
            text: "Inspect"
            onClicked: {
                if (registries.objects_registry.has(object_select.currentValue)) {
                    root.spawn_window(registries.objects_registry.get(object_select.currentValue))
                }
            }
        }
    }
}