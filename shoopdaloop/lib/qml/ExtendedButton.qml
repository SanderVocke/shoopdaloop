import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3

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