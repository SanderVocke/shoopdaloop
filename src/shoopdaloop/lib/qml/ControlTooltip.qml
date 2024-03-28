import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3

Item {
    id: root
    property string text : ""
    anchors.fill: parent

    ToolTip {
        delay: 1000
        visible: ma.containsMouse && root.text != ""
        text: root.text
    }
    MouseArea {
        id: ma
        hoverEnabled: true
        anchors.fill: parent
        acceptedButtons: Qt.NoButton
    }
}