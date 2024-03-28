import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6

// Two reason for this component:
// - no way to share mouse events between controls and mouseareas apparently.
Item {
    id: root
    property bool hovered

    function onMousePosition(_pt) {
        var pt = mapFromGlobal(_pt.x, _pt.y)
        var inside = pt.x >= 0.0 && pt.x <= width && pt.y >= 0.0 && pt.y <= height
        root.hovered = inside
    }
    function onMouseExited() {
        root.hovered = false
    }
}