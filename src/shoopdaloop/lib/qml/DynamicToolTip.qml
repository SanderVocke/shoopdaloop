import QtQuick 6.6
import QtQuick.Controls 6.6

FirstTimeLoader {
    property alias delay: tooltip.delay
    property bool visible
    property alias text: tooltip.text

    activate: visible

    sourceComponent: ToolTip {
        id: tooltip
    }
}