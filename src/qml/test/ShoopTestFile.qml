import QtQuick 6.6
import QtQuick.Controls 6.6
import ShoopDaLoop.Rust
import '..'

Window {

    TestRunner { id: runner }

    Component.onCompleted: {
        ShoopCrashHandling.set_json_tag("shoop_phase", "runtime")
        ShoopTestFileRunner.testcase_runner = runner
    }
    Component.onDestruction: {
        ShoopCrashHandling.set_json_tag("shoop_phase", "quit")
    }
}