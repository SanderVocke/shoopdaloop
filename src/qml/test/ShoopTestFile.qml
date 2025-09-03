import QtQuick 6.6
import QtQuick.Controls 6.6
import ShoopDaLoop.Rust
import '..'

Window {

    TestRunner {
        id: runner
        objectName: "TestRunner"
        filter_regex: shoop_test_file_runner.test_filter_pattern
        onTestcase_done: {
            shoop_test_file_runner.on_testcase_done()
        }
        Component.onCompleted: {
            shoop_test_file_runner.testcase_runner = runner
        }
        Component.onDestruction: {
            shoop_test_file_runner.testcase_runner = null
        }
    }

    Component.onCompleted: {
        ShoopRustCrashHandling.set_json_tag("shoop_phase", "runtime")
        
    }
    Component.onDestruction: {
        ShoopRustCrashHandling.set_json_tag("shoop_phase", "quit")
    }
}