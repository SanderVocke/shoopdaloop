import QtQuick 2.3
import QtTest 1.0

import '../../../build/types.js' as Types
import '../../generate_session.js' as GenerateSession
import '..'

TestCase {
    id: testcase
    name: "Backend"

    // Backend {
    //     id: backend
    //     update_interval_ms: 30
    //     client_name_hint: 'ShoopDaLoop'
    //     backend_type: Types.BackendType.Dummy

    //     Session {
    //         anchors.fill: parent
    //         initial_descriptor: GenerateSession.generate_default_session()
    //     }
    // }

    function test_default_session() {
        console.log("Hi!")
    }

    function cleanupTestCase() {
        console.log("Cleanup!")
        //backend.close()
    }
}