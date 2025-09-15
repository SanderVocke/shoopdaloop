import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6

ToolbarButtonBase {
    id: root
    implicitWidth: text ? label.width + 6 : size

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
}