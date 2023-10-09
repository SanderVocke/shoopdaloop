import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15
import QtQuick.Dialogs

import ".."
import "../../generated/types.js" as Types

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
            backend_type: Types.BackendType.Jack

            anchors.fill: parent

            TracksWidget {
                anchors.fill: parent

                initial_track_descriptors: []
                objects_registry: registries.objects_registry
                state_registry: registries.state_registry
            }
        }
    }
}
