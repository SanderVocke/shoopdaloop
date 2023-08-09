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

    component ItemRow: Row {
        spacing: 5
        property alias label: l.text
        Label { id: l; width: 200 }
    }

    Component {
        id: debug_port_component
        Item {
            Column {
                anchors.fill: parent
                spacing: 5

                ItemRow {
                    label: "obj_id:"
                    Label { text: object.obj_id }
                }

                ItemRow {
                    label: "object_schema:"
                    Label { text: object.object_schema }
                }

                ItemRow {
                    label: "muted:"
                    Label { text: object.muted }
                }

                ItemRow {
                    label: "passthrough muted:"
                    Label { text: object.passthrough_muted }
                }

                ItemRow {
                    label: "direction:"
                    Label { text: object.direction }
                }

                ItemRow {
                    label: "internal:"
                    Label { text: object.is_internal }
                }

                ItemRow {
                    label: "passthrough to:"
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

                Loader {
                    active: object.object_schema.match(/audioport.[0-9]/)
                    sourceComponent: ItemRow {
                        label: "volume:"
                        Label { text: object.volume.toString() }
                    }
                }

                Loader {
                    active: object.object_schema.match(/midiport.[0-9]/)
                    sourceComponent: ItemRow {
                        label: "n notes active: "
                        Label { text: object.n_notes_active.toString() }
                    }
                }

                ItemRow {
                    label: "descriptor:"
                    Label { text: JSON.stringify(object.descriptor, null, 2) }
                }
            }
        }
    }
}