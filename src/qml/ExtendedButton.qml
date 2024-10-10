import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6

ShoopButton {
    id: root
    property string tooltip: ""

    ControlTooltip {
        text: root.tooltip
    }
}