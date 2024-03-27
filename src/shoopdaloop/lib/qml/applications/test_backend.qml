import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6
import QtQuick.Dialogs

import ".."
import "../../generate_session.js" as GenerateSession
import ShoopConstants

ApplicationWindow {
    visible: true
    width: 1050
    height: 550
    minimumWidth: 500
    minimumHeight: 350
    title: "ShoopDaLoop"

    Material.theme: Material.Dark

    Label {
        text: '(Backend test, close when done)'
    }

    Backend {
        update_interval_ms: 30
        client_name_hint: 'ShoopDaLoop'
        backend_type: ShoopConstants.AudioDriverType.Jack
        id: backend
    }
}
