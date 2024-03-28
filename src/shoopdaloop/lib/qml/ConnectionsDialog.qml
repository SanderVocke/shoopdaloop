import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6
import QtQuick.Layouts
import ShoopDaLoop.PythonLogger

Dialog {
    property var audio_in_ports: []
    property var audio_out_ports: []
    property var audio_send_ports: []
    property var audio_return_ports: []
    property var midi_in_ports: []
    property var midi_out_ports: []
    property var midi_send_ports: []

    readonly property PythonLogger logger : PythonLogger { name:"Frontend.ConnectionsDialog" }

    id: root
    modal: true
    standardButtons: Dialog.Close

    width: Overlay.overlay ? Overlay.overlay.width - 50 : 800
    height: Overlay.overlay ? Overlay.overlay.height - 50 : 500
    anchors.centerIn: Overlay.overlay

    property var contents: ({
        'Audio in': root.audio_in_ports,
        'Audio out': root.audio_out_ports,
        'Audio send': root.audio_send_ports,
        'Audio return': root.audio_return_ports,
        'Midi in': root.midi_in_ports,
        'Midi out': root.midi_out_ports,
        'Midi send': root.midi_send_ports
    })

    TabBar {
        id: bar
        width: parent.width
        height: 50
        Repeater {
            model: Object.keys(root.contents).filter(k => root.contents[k].length > 0)
            TabButton {
                text: modelData
            }
        }
    }

    StackLayout {
        width: parent.width
        anchors.bottom: parent.bottom
        anchors.top: bar.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        currentIndex: bar.currentIndex

        Repeater {
            model: Object.values(root.contents).filter(v => v.length > 0)
            PortsConnections {
                ports: modelData
            }
        }
    }

    component PortsConnections: Item {
        id: connections
        property var ports: []
        property int name_width: 300
        property int column_width: 200
        property int grid_cell_size: 25
        property int font_size: 14

        Timer {
            interval: 100
            repeat: true
            running: parent.visible
            onTriggered: parent.update()
        }

        property var port_connections : []

        function update() {
            port_connections = ports.map(p => {
                return p.get_connections_state()
            })
        }

        onPortsChanged: {
            update()
        }

        readonly property var external_ports : {
            var all_ports = []
            for(var idx=0; idx < port_connections.length; idx++) {
                for(var j=0; j < Object.entries(port_connections[idx]).length; j++) {
                    let name = Object.entries(port_connections[idx])[j][0]
                    all_ports.push(name)
                }
            }

            // Uniquify
            all_ports = Array.from(new Set(all_ports))

            return all_ports
        }

        // Header row for our ports
        Row {
            id: header
            anchors.top: parent.top
            height: 150

            Rectangle {
                width: connections.name_width
                height: 150
                color: 'transparent'
            }
            Mapper {
                model: connections.ports
                
                Rectangle {
                    property int index
                    property var mapped_item
                    width: connections.grid_cell_size
                    height: 150
                    color: 'transparent'
                    Label {
                        id: label
                        text: {
                            let parts = mapped_item.name.split(':')
                            if (parts.length == 2) {
                                return parts[1]
                            } else {
                                return mapped_item.name
                            }
                        }
                        font.pixelSize: connections.font_size
                        anchors.right: parent.horizontalCenter
                        anchors.bottom: parent.bottom
                        transform: Rotation { origin.x: label.width; origin.y: label.height; angle: 25}
                    }
                }
            }
        }   

        ScrollView {
            anchors.bottom: parent.bottom
            anchors.top: header.bottom
            anchors.left: parent.left
            anchors.right: parent.right

            ScrollBar.vertical.policy: ScrollBar.AsNeeded
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff

            contentHeight: colly.height + 100
    
            Column {
                id: colly
                
                // Row per external port
                Mapper {
                    id: external_ports_mapper
                    model: connections.external_ports

                    Column {
                        id: external_port_item
                        property var mapped_item
                        property int index

                        Label {
                            property bool show: {
                                if (parent.index == 0) { return true; }
                                let maybe_previous = external_ports_mapper.model[parent.index-1];
                                let maybe_this = external_ports_mapper.model[parent.index];

                                // Show a label grouping ports of the same client.
                                return (maybe_this && maybe_previous && maybe_this.split(':')[0] != maybe_previous.split(':')[0]) ? true : false
                            }
                            height: show ? 30 : 0
                            text: show ? external_port_item.mapped_item.split(':')[0] : ""
                            font.pixelSize : connections.font_size
                            font.italic : true
                            font.underline: true

                            width: connections.name_width
                            horizontalAlignment: Text.AlignRight
                            verticalAlignment: Text.AlignBottom
                            rightPadding: 10
                        }

                        Row {
                            id: external_port_row

                            Rectangle {
                                width: connections.name_width
                                height: connections.grid_cell_size
                                color: 'transparent'

                                Label {
                                    anchors.verticalCenter: parent.verticalCenter
                                    anchors.right: parent.right
                                    anchors.rightMargin: 5
                                    font.pixelSize: connections.font_size
                                    text: mapped_item.split(':')[1]
                                }
                            }
                            Mapper {
                                model: connections.ports
                                
                                Rectangle {
                                    property int index
                                    property var mapped_item
                                    width: connections.grid_cell_size
                                    height: connections.grid_cell_size
                                    border.width: 1
                                    border.color: Material.foreground
                                    color: 'transparent'

                                    property var connected: connections.port_connections &&
                                                               connections.port_connections.length > index &&
                                                               connections.port_connections[index] &&
                                                               connections.port_connections[index].hasOwnProperty(external_port_item.mapped_item) &&
                                                               connections.port_connections[index][external_port_item.mapped_item]
                                    property var connectable : connections.port_connections &&
                                                               connections.port_connections.length > index &&
                                                               connections.port_connections[index] &&
                                                               connections.port_connections[index].hasOwnProperty(external_port_item.mapped_item)

                                    MouseArea {
                                        hoverEnabled: false
                                        anchors.fill: parent
                                        onClicked: {
                                            if (parent.connectable) {
                                                if (connected) {
                                                    parent.mapped_item.disconnect_external_port(external_port_item.mapped_item)
                                                } else {
                                                    parent.mapped_item.connect_external_port(external_port_item.mapped_item)
                                                }
                                                connections.update()
                                            }
                                        }
                                    }

                                    MaterialDesignIcon {
                                        size: 20
                                        name: (!connectable) ? "cancel" : "circle"
                                        visible: connected || !connectable
                                        color: Material.foreground

                                        anchors.centerIn: parent
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
