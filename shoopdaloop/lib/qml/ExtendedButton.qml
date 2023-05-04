import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

Button {
    id: root
    property string tooltip: ""

    ToolTip {
        delay: 1000
        visible: ma.containsMouse && tooltip != ""
        text: root.tooltip
    }
    MouseArea {
        id: ma
        hoverEnabled: true
        anchors.fill: parent
        acceptedButtons: Qt.NoButton
    }
}