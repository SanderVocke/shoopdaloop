import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

Rectangle {
    Drag.active: dragArea.drag.active
    Drag.hotSpot.x: width/2
    Drag.hotSpot.y: height/2
    Drag.onDragFinished: { console.log ("icon dropped")}

    required property var draggable_type
    required property var draggable_func
    property var drag_area: dragArea

    MaterialDesignIcon {
        name: 'record'
        color: 'red'
        anchors.fill: parent
    }

    MouseArea {
        id: dragArea
        anchors.fill: parent
        onReleased: {
            console.log("release")
            parent.Drag.drop()
            parent.destroy()
        }
        drag.target: parent
    }
}