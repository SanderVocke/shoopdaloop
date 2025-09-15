import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6

Rectangle {
    id: root
    property int size : 30

    implicitWidth: size
    implicitHeight: size

    border.color: "#777777"
    border.width: 1

    color: "transparent"
}