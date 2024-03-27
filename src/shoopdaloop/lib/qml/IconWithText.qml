import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6

Item {
    id: item

    property int size
    property alias name: material.name
    property alias color: material.color
    property alias font: text.font
    property alias text_color: text.color
    property alias text: text.text

    default property alias contents: children_container.children

    width: size
    height: size

    MaterialDesignIcon {
        id: material
        size: parent.size
        anchors.centerIn: parent
        name: 'record'
        color: 'red'
    }

    // Children go here so the text is always on top
    Item {
        anchors.fill: parent
        id: children_container
    }

    Text {
        id: text
        anchors.fill: parent
        font.pixelSize: item.size / 3.0
        horizontalAlignment: Text.AlignRight
        verticalAlignment: Text.AlignBottom
        rightPadding: 1
        bottomPadding: 1
        font.weight: Font.Bold
    }
}