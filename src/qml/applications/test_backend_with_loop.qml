import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6
import QtQuick.Dialogs 6.6
import ShoopDaLoop.Rust
import ".."
import "../js/generate_session.js" as GenerateSession


ShoopApplicationWindow {
    visible: true
    width: 1050
    height: 550
    minimumWidth: 500
    minimumHeight: 350
    title: "ShoopDaLoop"

    Label {
        text: '(Backend test, close when done)'
    }

     AppRegistries {
        id: registries
        anchors.fill:parent

        Backend {
            update_interval_ms: 30
            client_name_hint: 'ShoopDaLoop'
            backend_type: ShoopRustConstants.AudioDriverType.Jack
            id: backend

            Loop {
            }
        }
    }
}
