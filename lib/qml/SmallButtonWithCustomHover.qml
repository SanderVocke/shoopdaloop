import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

// Two reasons for this component:
// - the built-in button acts strange in terms of sizing when small. This adds some offset so that things work
// - no way to share mouse events between buttons and mouseareas apparently. so this has a mousearea instead
//   of the Button's one.
Rectangle {
    id: root
    property bool hovered

    signal clicked()

    color: hovered ? 'grey' : 'transparent'

    MouseArea {
        anchors.fill: parent
        onClicked: root.clicked()
    }

    function onMousePosition(_pt) {
        var pt = mapFromGlobal(_pt.x, _pt.y)
        var inside = pt.x >= 0.0 && pt.x <= width && pt.y >= 0.0 && pt.y <= height
        root.hovered = inside
    }
    function onMouseExited() {
        root.hovered = false
    }
}