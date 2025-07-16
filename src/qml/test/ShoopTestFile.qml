import QtQuick 6.6
import QtQuick.Controls 6.6

Window {
    Component.onCompleted: {
        ShoopCrashHandling.set_json_tag("shoop_phase", "runtime")
    }
    Component.onDestruction: {
        ShoopCrashHandling.set_json_tag("shoop_phase", "quit")
    }
}