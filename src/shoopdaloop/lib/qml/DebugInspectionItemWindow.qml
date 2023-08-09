import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3

ApplicationWindow {
    
    id: root
    title: "Inspect " + object ? object.obj_id : "Unknown"

    property var objects_registry: null
    property var object: null

    width: 200
    height: 500
    minimumWidth: 100
    minimumHeight: 100
    
    Material.theme: Material.Dark

    Loader {
        sourceComponent: {
            if (object && object.object_schema.match(/(?:audio|midi)port.[0-9]+/)) {
                return debug_port_component;
            }
            return null;
        }
        anchors.fill: parent
        active: true
    }

    readonly property var window_factory : Qt.createComponent("DebugInspectionItemWindow.qml")
    function spawn_window(object) {
        if (window_factory.status == Component.Error) {
            throw new Error("DebugInspectionMainWindow: Failed to load window factory: " + window_factory.errorString())
        } else if (window_factory.status != Component.Ready) {
            throw new Error("DebugInspectionMainWindow: Factory not ready")
        } else {
            var window = window_factory.createObject(root.parent, {
                object: object,
                visible: true
            })
        }
    }

    Component {
        id: debug_port_component
        Item {
            Grid {
                anchors.fill: parent
                columns: 2
                spacing: 5

                Label { text: "obj_id:" }
                Label { text: object.obj_id }

                Label { text: "object_schema:" }
                Label { text: object.object_schema }

                Label { text: "muted:" }
                Label { text: object.muted }

                Label { text: "passthrough muted:" }
                Label { text: object.passthrough_muted }

                Label { text: "direction:" }
                Label { text: object.direction }

                Label { text: "internal:" }
                Label { text: object.is_internal }

                Label { text: "passthrough to:" }
                Item {
                    Row {
                        spacing: 3
                        Mapper {
                            model: object.passthrough_to
                            Text {
                                property var mapped_item
                                property int index
                                text: "<a href=\"bla\">" + mapped_item.obj_id + "</a>"
                                onLinkActivated: root.spawn_window(mapped_item)
                                color: Material.foreground
                            }
                        }
                    }
                }

                Label { text: "descriptor:" }
                Label { text: JSON.stringify(object.descriptor, null, 2) }
            }
        }
    }
}