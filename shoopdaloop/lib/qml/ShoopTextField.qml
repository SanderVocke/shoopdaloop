import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

TextField {
    id: root
    font.pixelSize: 13
    leftInset: 2
    rightInset: 2
    bottomInset: 2
    topInset: 2

    background: Rectangle {
        implicitWidth: 120
        implicitHeight: 40
        color: "#555555"
        border.color: "grey"
        radius: 2
    }
}