import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15
import QtQuick.Dialogs

import ".."
import ShoopConstants

ApplicationWindow {
    id: root
    visible: true
    width: 1200
    height: 600
    maximumWidth: width
    maximumHeight: height
    minimumWidth: width
    minimumHeight: height
    title: "ShoopDaLoop Tracks"

    Material.theme: Material.Dark
    
    AppRegistries {
        id: registries
        anchors.fill:parent

        Backend {
            id: backend
            update_interval_ms: 30
            client_name_hint: 'ShoopDaLoop_Tracks'
            backend_type: ShoopConstants.AudioDriverType.Jack

            anchors.fill: parent

            TracksWidget {
                anchors.fill: parent

                initial_track_descriptors: []
            }
        }
    }
}
