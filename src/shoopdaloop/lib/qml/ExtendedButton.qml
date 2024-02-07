import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3

ShoopButton {
    id: root
    property string tooltip: ""

    ControlTooltip {
        text: root.tooltip
    }
}