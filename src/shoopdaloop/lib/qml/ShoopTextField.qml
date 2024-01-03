import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3

TextField {
    id: root
    font.pixelSize: 12
    leftInset: 0
    rightInset: 0
    bottomInset: 0
    topInset: 0

    leftPadding: 3
    rightPadding: 3
    bottomPadding: -1
    topPadding: -1

    background: Rectangle {
        implicitWidth: 120
        implicitHeight: 30
        color: "#555555"
        border.color: "grey"
        radius: 2
    }
}