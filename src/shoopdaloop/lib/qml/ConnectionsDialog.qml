import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3

Dialog {
    property var audio_in_ports: []
    property var audio_out_ports: []
    property var midi_in_ports: []
    property var midi_out_ports: []

    id: root
    modal: true
    standardButtons: Dialog.Close

    width: 800
    height: 400

    Button {
        text: "Print"
        onClicked: {
            if (audio_out_ports.length > 0) {
                console.log(audio_out_ports[0].get_connections_state())
            }
        }
    }
}
