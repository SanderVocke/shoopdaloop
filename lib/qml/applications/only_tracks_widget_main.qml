import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15
import QtQuick.Dialogs

import ".."

ApplicationWindow {
    visible: true
    width: 1200
    height: 600
    maximumWidth: width
    maximumHeight: height
    minimumWidth: width
    minimumHeight: height
    title: "ShoopDaLoop Tracks Widget"
    id: appWindow

    Material.theme: Material.Dark

    // Ensure other controls lose focus when clicked outside
    MouseArea {
        id: background_focus
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        onClicked: () => { forceActiveFocus() }
    }

    Backend {
        id: backend
        
        client_name_hint: "tracks_widget"
        update_interval_ms: 30

        TracksWidget {
            slots_per_track: 8
            n_tracks: 8
        }
    }
}
