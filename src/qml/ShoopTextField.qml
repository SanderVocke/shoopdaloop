import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6

TextField {
    id: root
    font.pixelSize: 12
    leftInset: 2
    rightInset: 2
    bottomInset: 2
    topInset: 2

    leftPadding: 8
    rightPadding: 6
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