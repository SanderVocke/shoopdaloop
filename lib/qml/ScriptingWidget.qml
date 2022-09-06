import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

Rectangle {
    id: widget
    color: "#555555"

    property var items : [1, 2, 3]

    Item {
        anchors.fill: parent
        anchors.margins: 6

        Item {
            anchors {
                top: parent.top
                bottom: parent.bottom
                left: parent.left
            }
            width: 80

            id: scriptingcontrol

            Text {
                id: scriptingtext

                anchors {
                    top: parent.top
                    left: parent.left
                    right: parent.right
                }

                text: "Scripting"
                font.pixelSize: 15
                color: Material.foreground
                verticalAlignment: Text.AlignTop
                anchors {
                    top: parent.top
                    bottom: parent.bottom
                }
            }
        }

        Rectangle {
            anchors {
                top: parent.top
                bottom: parent.bottom
                left: scriptingcontrol.right
                leftMargin: 6
                right: parent.right
            }
            width: 360
            border.color: 'grey'
            border.width: 1
            color: 'transparent'

            ScrollView {
                anchors {
                    fill: parent
                    margins: parent.border.width + 1
                }

                ScrollBar.horizontal.policy: ScrollBar.AsNeeded
                ScrollBar.vertical.policy: ScrollBar.AlwaysOff

                Row {
                    spacing: 1
                    anchors.fill: parent

                    Repeater {
                        model: widget.items.length
                        anchors.fill: parent

                        ScriptItemWidget {
                            anchors {
                                top: parent.top
                                bottom: parent.bottom
                            }

                            width: 40
                        }
                    }
                }
            }
        }
    }

    component ScriptItemWidget : Rectangle {
        color: 'black'
    }
}
