import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15
import QtQuick.Dialogs

import ".."
import ShoopConstants
import "../../generate_session.js" as GenerateSession

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

            TracksWidget {
                anchors.fill: parent

                initial_track_descriptors: [
                    GenerateSession.generate_default_track('a', 4, 'a', false, 'a'),
                    GenerateSession.generate_default_track('b', 4, 'b', false, 'b'),
                    GenerateSession.generate_default_track('c', 4, 'c', false, 'c')
                ]

                onLoadedChanged: Qt.quit()
            }
    }
}
