import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6
import QtQuick.Dialogs 6.6

import ".."
import ShoopConstants

ShoopApplicationWindow {
    id: root
    visible: true
    width: 1200
    height: 600
    maximumWidth: width
    maximumHeight: height
    minimumWidth: width
    minimumHeight: height
    title: "ShoopDaLoop Tracks"
    
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
