import QtQuick 6.6
import QtQuick.Controls 6.6

FirstTimeLoader {
    id: root
    property int delay
    property string text

    activate: visible

    sourceComponent: ToolTip {
        id: tooltip
        delay: root.delay
        text: root.text
    }
}