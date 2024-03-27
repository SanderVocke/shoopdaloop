import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6

Rectangle {
    id: root
    property int size : 30

    implicitWidth: text ? label.width + 6 : size
    implicitHeight: size

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

    border.color: "#777777"
    border.width: 1

    signal clicked()

    color : {
        if (pressed_color && pressed) { return pressed_color }
        if (hovered_color && hovered) { return hovered_color }
        if (toggle_visual_active) { return pressed_color }
        return regular_color
    }

    Loader {
        active: parent.material_design_icon != null
        anchors.centerIn: parent
        sourceComponent: Component {
            MaterialDesignIcon {
                anchors.centerIn: parent
                size: root.size
                name: root.material_design_icon
                color: Material.foreground
            }
        }
    }

    Label {
        id: label
        visible: root.text
        text: root.text
        color: Material.foreground
        anchors.centerIn: parent
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