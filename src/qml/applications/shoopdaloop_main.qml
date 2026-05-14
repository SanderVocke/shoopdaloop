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

    // Timer to trigger intentional panic for crash handler testing
    Timer {
        id: panicTimer
        // panic_after is passed from Rust via global_args (in seconds, 0 or negative means disabled)
        interval: (global_args.panic_after > 0) ? global_args.panic_after * 1000 : 0
        running: interval > 0
        onTriggered: {
            console.log("Triggering intentional panic for crash handler testing...")
            ShoopRustOSUtils.cause_panic()
        }
    }

    Session {
        anchors.fill: parent
        initial_descriptor: GenerateSession.generate_default_session(global_args.version_string, null, true)
        settings_io_enabled: true

        onLoadedChanged: {
            if (loaded) {
                ShoopRustCrashHandling.set_json_tag("shoop_phase", "runtime")
            }
        }
    }
}
