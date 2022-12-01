import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

Dialog {
    id: dialog
    modal: true
    title: 'Settings'
    standardButtons: Dialog.Close

    width: 800
    height: 450

    Column {
        anchors.fill: parent
        TabBar {
            id: bar
            anchors.left: parent.left
            anchors.right: parent.right

            TabButton {
                text: 'MIDI control'
            }
            TabButton {
                text: 'JACK'
            }
        }
    }
}
