import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

MaterialDesignIcon {
    name: 'record'
    color: 'red'
    
    Drag.active: dragArea.drag.active
    Drag.hotSpot.x: 10
    Drag.hotSpot.y: 10
    Drag.onDragFinished: { console.log ("icon dropped")}

    required property var draggable_type
    required property var draggable_func

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