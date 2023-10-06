import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3

ApplicationWindow {
    
    id: root
    title: "Inspect " + object ? object.obj_id : "Unknown"

    property var objects_registry: null
    property var object: null

    width: 500
    height: 300
    minimumWidth: 100
    minimumHeight: 100
    
    Material.theme: Material.Dark

    ScrollView {
        anchors.fill: parent
        contentWidth: 400
        contentHeight: 1600
        ScrollBar.horizontal.policy: ScrollBar.AlwaysOn
        ScrollBar.vertical.policy: ScrollBar.AlwaysOn

        Loader {
            id: loader
            sourceComponent: {
                if (object && object.object_schema.match(/(?:audio|midi)port.[0-9]+/)) {
                    return debug_port_component;
                } else if (object && object.object_schema.match(/channel.[0-9]+/)) {
                    return debug_channel_component;
                } else if (object && object.object_schema.match(/loop.[0-9]+/)) {
                    return debug_loop_component;
                } else if (object && object.object_schema.match(/fx_chain.[0-9]+/)) {
                    return debug_fx_chain_component;
                }
                return null;
            }
            active: true
        }
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
                spacing: 3

                ItemRow {
                    label: "obj_id:"
                    Label { text: object.obj_id }
                }

                ItemRow {
                    label: "name:"
                    Label { text: object.name }
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

    Component {
        id: debug_channel_component
        Item {
            Column {
                anchors.fill: parent
                spacing: 3

                ItemRow {
                    label: "obj_id:"
                    Label { text: object.obj_id }
                }

                ItemRow {
                    label: "object_schema:"
                    Label { text: object.object_schema }
                }

                ItemRow {
                    label: "initialized:"
                    Label { text: object.initialized }
                }

                ItemRow {
                    label: "loop:"
                    Text {
                        text: object.loop ? "<a href=\"bla\">" + object.loop.obj_id + "</a>" : "none"
                        onLinkActivated: root.spawn_window(object.loop)
                        color: Material.foreground
                    }
                }

                ItemRow {
                    label: "recording started:"
                    Label { text: object.recording_started_at }
                }

                ItemRow {
                    label: "mode:"
                    Label { text: object.mode }
                }

                ItemRow {
                    label: "length:"
                    Label { text: object.data_length }
                }

                ItemRow {
                    label: "last played:"
                    Label { text: object.played_back_sample }
                }

                ItemRow {
                    label: "start offset:"
                    Label { text: object.start_offset }
                }

                ItemRow {
                    label: "n preplay:"
                    Label { text: object.n_preplay_samples }
                }

                ItemRow {
                    label: "dirty:"
                    Label { text: object.data_dirty }
                }

                ItemRow {
                    label: "connected ports:"
                    Row {
                        spacing: 3
                        Mapper {
                            model: object.connected_ports
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

                ItemRow {
                    label: "ports to conn:"
                    Row {
                        spacing: 3
                        Mapper {
                            model: object.ports_to_connect
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
                    active: object.descriptor.type == "audio"
                    sourceComponent: ItemRow {
                        label: "volume:"
                        Label { text: object.volume.toString() }
                    }
                }

                Loader {
                    active: object.descriptor.type == "midi"
                    sourceComponent: ItemRow {
                        label: "notes active:"
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

    Component {
        id: debug_loop_component
        Item {
            Column {
                anchors.fill: parent
                spacing: 3

                ItemRow {
                    label: "obj_id:"
                    Label { text: object.obj_id }
                }

                ItemRow {
                    label: "object_schema:"
                    Label { text: object.object_schema }
                }

                ItemRow {
                    label: "initialized:"
                    Label { text: object.initialized }
                }

                ItemRow {
                    label: "mode:"
                    Label { text: object.mode }
                }

                ItemRow {
                    label: "length:"
                    Label { text: object.length }
                }

                ItemRow {
                    label: "position:"
                    Label { text: object.position }
                }

                ItemRow {
                    label: "next_mode:"
                    Label { text: object.next_mode }
                }

                ItemRow {
                    label: "transition delay:"
                    Label { text: object.next_transition_delay }
                }

                ItemRow {
                    label: "sync source:"
                    Text {
                        text: object.sync_source ? "<a href=\"bla\">" + object.sync_source.obj_id + "</a>" : "none"
                        onLinkActivated: root.spawn_window(object.sync_source)
                        color: Material.foreground
                    }
                }

                ItemRow {
                    label: "descriptor:"
                    Label { text: JSON.stringify(object.initial_descriptor, null, 2) }
                }
            }
        }
    }

    Component {
        id: debug_fx_chain_component
        Item {
            Column {
                anchors.fill: parent
                spacing: 3

                ItemRow {
                    label: "obj_id:"
                    Label { text: object.obj_id }
                }

                ItemRow {
                    label: "object_schema:"
                    Label { text: object.object_schema }
                }

                ItemRow {
                    label: "initialized:"
                    Label { text: object.initialized }
                }

                ItemRow {
                    label: "ui visible:"
                    Label { text: object.ui_visible }
                }

                ItemRow {
                    label: "title:"
                    Label { text: object.title }
                }

                ItemRow {
                    label: "ready:"
                    Label { text: object.ready }
                }

                ItemRow {
                    label: "active:"
                    Label { text: object.active }
                }

                ItemRow {
                    label: "type:"
                    Label { text: object.chain_type }
                }

                ItemRow {
                    label: "descriptor:"
                    Label { text: JSON.stringify(object.descriptor, null, 2) }
                }
            }
        }
    }
}