import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3
import QtQuick.Dialogs

import ".."
import "../../generate_session.js" as GenerateSession
import "../../generated/types.js" as Types

ApplicationWindow {
    visible: true
    width: 1050
    height: 550
    minimumWidth: 500
    minimumHeight: 350
    title: "ShoopDaLoop"
    id: appWindow

    Material.theme: Material.Dark

    Label {
        text: '(Backend test, close when done)'
    }

     AppRegistries {
        id: registries
        anchors.fill:parent

        Backend {
            update_interval_ms: 30
            client_name_hint: 'ShoopDaLoop'
            backend_type: Types.BackendType.Jack
            id: backend

            DynamicLoop {
                id: dynamic_loop
                force_load : true
            }
        }
    }
}
