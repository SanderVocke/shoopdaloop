import QtQuick 6.6
import QtQuick.Controls 6.6
import ShoopDaLoop.Rust
import '..'

Window {

    TestRunner {
        id: runner
        onTestcase_done: {
            shoop_test_file_runner.on_testcase_done()
        }
    }

    Component.onCompleted: {
        ShoopCrashHandling.set_json_tag("shoop_phase", "runtime")
        shoop_test_file_runner.testcase_runner = runner
    }
    Component.onDestruction: {
        console.log("DELETING")
        ShoopCrashHandling.set_json_tag("shoop_phase", "quit")
    }
}