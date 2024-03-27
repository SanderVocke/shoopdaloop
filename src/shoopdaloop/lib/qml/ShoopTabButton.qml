import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6

Button {
    id: root

    property var background_color : selected ? "#555555" : "#333333"
    property var tabbar : parent

    // The representative identifies the button uniquely to the tabbar.
    property var representative : this

    property bool selected : tabbar.selected_button == this
    property bool auto_select : true
    property bool closeable : true
    onClicked : tabbar.select(this)

    signal close()

    width: label.width + (root.closeable ? 24 : 16)
    height: 30

    contentItem : Item {
        id: content
        Label {
            id: label
            text: root.text
            font.pixelSize: 12

            x: -18
            y: 0

            topPadding: -4
        }

        Item {
            width: closeicon.width
            height: closeicon.height
            visible: root.closeable
            MouseArea {
                anchors.fill: parent
                onClicked: root.close()
            }
            anchors {
                left: label.right
                leftMargin: 3
                verticalCenter: label.verticalCenter
            }
            MaterialDesignIcon {
                id: closeicon
                size: 12
                anchors.centerIn: parent
                name: 'close'
                color: Material.foreground
            }
        }
    }

    background: Rectangle {
        color: root.background_color
        radius: 3
        height: root.height

        // Cover up the radius on the bottom side
        Rectangle {
            color: parent.color
            anchors.fill: parent
            anchors.topMargin: parent.radius
        }
    }
}