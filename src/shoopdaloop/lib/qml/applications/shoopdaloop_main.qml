import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3
import QtQuick.Dialogs

import ".."
import "../../generate_session.js" as GenerateSession

ApplicationWindow {
    visible: true
    width: 1050
    height: 550
    minimumWidth: 500
    minimumHeight: 350
    title: "ShoopDaLoop"
    id: appWindow

    Material.theme: Material.Dark

    Session {
        anchors.fill: parent
        initial_descriptor: GenerateSession.generate_default_session(app_metadata.version_string)
        settings_io_enabled: true
    }
}
