import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6

ScrollView {
    id: root

    MouseArea {
        x: 0
        y: 0
        width: Math.max(root.contentWidth, root.width)
        height: Math.max(root.contentHeight, root.height)
        onClicked: ShoopReleaseFocusNotifier.notify()
    }
}