import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15
import QtQuick.Dialogs

ApplicationWindow {
    visible: true
    width:300
    height: 300
    maximumWidth: width
    maximumHeight: height
    minimumWidth: width
    minimumHeight: height
    title: "ShoopDaLoop Single Loop"
    id: appWindow

    Material.theme: Material.Dark

    // Ensure other controls lose focus when clicked outside
    MouseArea {
        id: background_focus
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        onClicked: () => { forceActiveFocus() }
    }

    LoopWidget {
        is_in_hovered_scene: false
        is_in_selected_scene: false
        name: "Loop"
        backend_loop: rootcontext_backend_loop
        master_backend_loop: rootcontext_backend_loop
        targeted_backend_loop: null
    }
}
