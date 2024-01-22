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

    Material.theme: Material.Dark

    Loader {
        RegistryLookup {
            id: lookup_general_settings
            registry: registries.state_registry
            key: 'general_settings'
        }
        property alias general_settings: lookup_general_settings.object
        active: general_settings && general_settings.embed_carla

        sourceComponent: Component{
            CarlaWaylandWrapper {
                RegisterInRegistry {
                    registry: registries.state_registry
                    key: 'carla_wayland_wrapper'
                    object: parent
                }
            }
        }
    }

    Session {
        anchors.fill: parent
        initial_descriptor: GenerateSession.generate_default_session(app_metadata.version_string, null, true)
        settings_io_enabled: true
    }
}
