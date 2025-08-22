import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6

ShoopApplicationWindow {
    id: root
    title: "Connections"

    width: 400
    height: 450

    property alias audio_in_ports: control.audio_in_ports
    property alias audio_out_ports: control.audio_out_ports
    property alias audio_send_ports: control.audio_send_ports
    property alias audio_return_ports: control.audio_return_ports
    property alias midi_in_ports: control.midi_in_ports
    property alias midi_out_ports: control.midi_out_ports
    property alias midi_send_ports: control.midi_send_ports

    ConnectionsControl {
        id: control
        anchors.fill: parent
    }
}