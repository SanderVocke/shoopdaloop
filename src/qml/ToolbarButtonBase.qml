import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6

ToolbarRectangle {
    id: root

    property var text : null
    property var material_design_icon : null
    property var hovered_color : "#666666"
    property var pressed_color : "#888888"
    property var regular_color : "transparent"
    property alias pressed: area.pressed
    property alias hovered: area.containsMouse

    property bool togglable : false
    property bool checked : false
    property bool toggle_visual_active : {
        return togglable && checked
    }

    signal clicked()

    color : {
        if (pressed_color && pressed) { return pressed_color }
        if (hovered_color && hovered) { return hovered_color }
        if (toggle_visual_active) { return pressed_color }
        return regular_color
    }

    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        onClicked: {
            root.clicked()
            if (root.togglable) {
                root.checked = !root.checked
            }
        }
    }
}