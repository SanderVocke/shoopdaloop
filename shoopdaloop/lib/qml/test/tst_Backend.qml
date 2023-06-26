import QtQuick 6.3
import QtTest 1.0
import Backend

import '../../backend/frontend_interface/types.js' as Types
import './testfilename.js' as TestFilename

Backend {
    id: backend
    update_interval_ms: 30
    client_name_hint: 'ShoopDaLoop'
    backend_type: Types.BackendType.Dummy

    ShoopTestCase {
        name: 'TwoLoops'
        filename : TestFilename.test_filename()

        function test_backend() {
            start_test_fn('test_backend')
            verify(backend.initialized)
            end_test_fn('test_backend')
        }

        function cleanupTestCase() { backend.close() }
    }
}