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

    width: 800
    height: 400

    TabBar {
        id: bar
        width: parent.width
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
        property int name_width: 100
        property int column_width: 100

        Row {
            Mapper {
                model: connections.ports
                Label {
                    property var mapped_item
                    property int index

                    text: mapped_item.name

                    Component.onCompleted: root.logger.info("Made label! " + text)
                    onTextChanged: root.logger.info(text)
                }
            }

            // Rectangle { width: connections.name_width }

            // Mapper {
            //     model: connections.ports
            
            //     Rectangle {
            //         property int index
            //         property var mapped_item

            //         width: connections.column_width
            //         Label {
            //             text: mapped_item.name
            //             transform: Rotation { origin.x: 0; origin.y: 0; angle: 45}
            //         }
            //     }
            // }
        }
        // Column {
        //     Mapper {
        //         model : ports

        //         Row {

        //         }
        //     }
        // }
    }
}
