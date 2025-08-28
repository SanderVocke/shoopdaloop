import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Dialogs 6.6
import ShoopDaLoop.Rust

import ".."
import "../js/generate_session.js" as GenerateSession

ShoopApplicationWindow {
    visible: true
    width: 1050
    height: 550
    minimumWidth: 350
    minimumHeight: 350
    title: "ShoopDaLoop"

    Session {
        anchors.fill: parent
        initial_descriptor: GenerateSession.generate_default_session(global_args.version_string, null, true)
        settings_io_enabled: true

        onLoadedChanged: {
            if (loaded) {
                ShoopCrashHandling.set_json_tag("shoop_phase", "runtime")
            }
        }
    }
}
