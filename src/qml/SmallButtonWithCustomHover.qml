import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6

// Two reasons for this component:
// - the built-in button acts strange in terms of sizing when small. This adds some offset so that things work
// - using the custom hover
Rectangle {
    id: root
    property alias hovered: hoverdetect.hovered

    CustomHoverDetection {
        id: hoverdetect
        anchors.fill: parent
    }

    signal clicked()
    signal pressAndHold(var mouse)
    
    color: hovered ? 'grey' : 'transparent'

    MouseArea {
        anchors.fill: parent
        onClicked: root.clicked()
        onPressAndHold: (mouse) => root.pressAndHold(mouse)
    }
    
    function onMousePosition(_pt) { hoverdetect.onMousePosition(_pt) }
    function onMouseExited() { hoverdetect.onMouseExited() }
}