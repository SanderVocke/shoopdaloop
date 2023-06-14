import QtQuick 6.3
import QtTest 1.0
import Backend

import '../../backend/frontend_interface/types.js' as Types

Backend {
    id: backend
    update_interval_ms: 30
    client_name_hint: 'ShoopDaLoop'
    backend_type: Types.BackendType.Dummy

    TestCase {
        function test_backend() {
            verify(backend.initialized)
        }

        function cleanupTestCase() { backend.close() }
    }
}