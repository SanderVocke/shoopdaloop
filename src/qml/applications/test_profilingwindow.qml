// ProfilingWindow {
//     backend: null
// }

import QtQuick 6.6
import QtQuick.Controls 6.6

ApplicationWindow {
    visible: true

    TreeView {
        id: tree
       
        rowHeightProvider: (idx) => 20

        anchors.fill: parent

        delegate: TreeViewDelegate {
            id: delegate
            background: Rectangle { color: 'transparent' }
        }

        model: null
        onModelChanged: expander.trigger()
    }
}