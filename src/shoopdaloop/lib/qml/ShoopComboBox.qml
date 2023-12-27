import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3

ComboBox {
    id: root
    font.pixelSize: 13
    leftInset: 2
    rightInset: 2
    bottomInset: 2
    topInset: 2

    background: Rectangle {
        implicitWidth: 120
        implicitHeight: 30
        color: "#555555"
        border.color: "grey"
        radius: 2
    }
}