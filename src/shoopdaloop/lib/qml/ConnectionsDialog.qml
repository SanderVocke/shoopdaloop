import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3
import QtQuick.Layouts
import ShoopDaLoop.PythonLogger

Dialog {
    property var audio_in_ports: []
    property var audio_out_ports: []
    property var midi_in_ports: []
    property var midi_out_ports: []

    readonly property PythonLogger logger : PythonLogger { name:"Frontend.ConnectionsDialog" }

    id: root
    modal: true
    standardButtons: Dialog.Close

    width: Overlay.overlay.width - 50
    height: Overlay.overlay.height - 50
    anchors.centerIn: Overlay.overlay

    TabBar {
        id: bar
        width: parent.width
        height: 50
        TabButton {
            text: 'Audio in'
        }
        TabButton {
            text: 'Audio out'
        }
        TabButton {
            text: 'Midi in'
        }
        TabButton {
            text: 'Midi out'
        }
    }

    StackLayout {
        width: parent.width
        anchors.bottom: parent.bottom
        anchors.top: bar.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        currentIndex: bar.currentIndex
        PortsConnections {
            id: audio_in_view
            ports: root.audio_in_ports
        }
        PortsConnections {
            id: audio_out_view
            ports: root.audio_out_ports
        }
        PortsConnections {
            id: midi_in_view
            ports: root.midi_in_ports
        }
        PortsConnections {
            id: midi_out_view
            ports: root.midi_out_ports
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
                        text: mapped_item.name
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
            ScrollBar.horizontal.policy: ScrollBar.AsNeeded
    
            Column {
                // Row per external port
                Mapper {
                    model: connections.external_ports

                    Row {
                        id: external_port_row
                        property var mapped_item
                        property int index

                        Rectangle {
                            width: connections.name_width
                            height: connections.grid_cell_size
                            color: 'transparent'

                            Label {
                                anchors.verticalCenter: parent.verticalCenter
                                anchors.right: parent.right
                                anchors.rightMargin: 5
                                font.pixelSize: connections.font_size
                                text: mapped_item
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

                                property var connected : {
                                    let our_port_conns = connections.port_connections[index]
                                    if (our_port_conns.hasOwnProperty (external_port_row.mapped_item)) {
                                        return our_port_conns[external_port_row.mapped_item]
                                    }
                                }
                                property var connectable : connections.port_connections[index].hasOwnProperty(external_port_row.mapped_item)

                                MouseArea {
                                    hoverEnabled: false
                                    anchors.fill: parent
                                    onClicked: {
                                        if (parent.connectable) {
                                            if (connected) {
                                                parent.mapped_item.disconnect_external_port(external_port_row.mapped_item)
                                            } else {
                                                parent.mapped_item.connect_external_port(external_port_row.mapped_item)
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
