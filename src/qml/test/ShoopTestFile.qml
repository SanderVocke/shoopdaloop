import QtQuick 6.6
import QtQuick.Controls 6.6
import ShoopDaLoop.Rust
import '..'

Window {

    TestRunner {
        id: runner
        filter_regex: shoop_test_file_runner.test_filter_pattern
        onTestcase_done: {
            shoop_test_file_runner.on_testcase_done()
        }
    }

    Component.onCompleted: {
        ShoopCrashHandling.set_json_tag("shoop_phase", "runtime")
        shoop_test_file_runner.testcase_runner = runner
    }
    Component.onDestruction: {
        ShoopCrashHandling.set_json_tag("shoop_phase", "quit")
    }
}