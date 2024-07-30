import QtQuick
import QtTest
import ShoopDaLoop.Rust

ShoopTestFile {
    ShoopTestCase {
        name: "OSUtils tests"

        function test_env_var_null() {
            compare(ShoopOSUtils.getEnvVar("DEADBEEF"), null, "null env var")
        }
    }
}
