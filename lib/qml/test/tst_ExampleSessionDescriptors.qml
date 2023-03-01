import QtQuick 2.3
import QtTest 1.0
import Backend

import '../../../build/types.js' as Types
import '../../generate_session.js' as GenerateSession
import '..'

Backend {
    id: backend
    update_interval_ms: 30
    client_name_hint: 'ShoopDaLoop'
    backend_type: Types.BackendType.Dummy

    Session {
        anchors.fill: parent
        initial_descriptor: GenerateSession.generate_default_session()
    }

    TestCase {
        function test_backend() {
            verify(backend.initialized)
        }

        function cleanupTestCase() { backend.close() }
    }
}