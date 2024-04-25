import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6

Item {
    id: root

    property real max_height: parent ? parent.height : 500
    property real min_height: 50
    property real max_width: parent ? parent.width: 500
    property real min_width: 50
    property real drag_area_thickness: 4

    property bool top_drag_enabled: false
    property int top_drag_area_y_offset: 0
    property bool bottom_drag_enabled: false
    property int bottom_drag_area_y_offset: 0

    MouseArea {
        z: 3
        id: resize_top_area
        height: root.drag_area_thickness
        enabled: root.top_drag_enabled
        visible: root.top_drag_enabled

        property real dist_from_top: root.top_drag_area_y_offset - height
        y: dist_from_top

        onYChanged: {
            if (drag.active) {
                let new_height = Math.min(Math.max(root.height - y + dist_from_top, root.min_height), root.max_height)
                root.height = new_height
                y = dist_from_top
            }
        }

        anchors {
            left: parent.left
            right: parent.right
        }

        cursorShape: Qt.SizeVerCursor

        drag {
            axis: "YAxis"
            target: resize_top_area
        }
    }

    MouseArea {
        z: 3
        id: resize_bottom_area
        height: root.drag_area_thickness
        enabled: root.bottom_drag_enabled
        visible: root.bottom_drag_enabled

        property real dist_from_bottom: root.bottom_drag_area_y_offset + height
        y: root.height - dist_from_bottom

        onYChanged: {
            if (drag.active) {
                let new_height = Math.min(Math.max(y + dist_from_bottom, root.min_height), root.max_height)
                root.height = new_height
                y = root.height - dist_from_bottom
            }
        }

        anchors {
            left: parent.left
            right: parent.right
        }

        cursorShape: Qt.SizeVerCursor

        drag {
            axis: "YAxis"
            target: resize_bottom_area
        }
    }
}
