import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6

ShoopSlider {
    id: root
    property real reset_value: 0.0
    TapHandler {
        onDoubleTapped: apply.trigger()
    }
    ExecuteNextCycle {
        id: apply
        onExecute: { root.value = reset_value; root.onMoved() }
    }

    property string tooltip: ""
    property bool show_value_tooltip: false
    property string value_tooltip_postfix: ""

    HoverHandler {
        id: hover
    }

    ToolTip {
        background: Rectangle { color: Material.background }
        x: root.handle.x - width/2
        y: -root.handle.height
        text: root.value.toFixed(1) + root.value_tooltip_postfix
        visible: hover.hovered && root.show_value_tooltip
        leftInset: 0
        rightInset: 0
        topInset: 0
        bottomInset: -10
        topPadding: 0
        bottomPadding: -10
        leftPadding: 0
        rightPadding: 0
    }

    ToolTip {
        delay: 1000
        visible: output_balance_ma.containsMouse && tooltip != ""
        text: root.tooltip
    }
    MouseArea {
        id: output_balance_ma
        hoverEnabled: true
        anchors.fill: parent
        acceptedButtons: Qt.NoButton
    }

    function gainToDb(gain) {
        return 20 * Math.log10(gain)
    }
}