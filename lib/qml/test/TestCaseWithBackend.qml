import QtQuick 2.3
import QtTest 1.0

import '../../../build/types.js' as Types

TestCase {
    id: testcase
    name: "Backend"
    property var backend

    function create_backend() {
        testcase.backend = createTemporaryQmlObject(
`import Backend;
Backend {
    update_interval_ms: 30
    client_name_hint: 'ShoopDaLoop'
    backend_type: ${Types.BackendType.Dummy}
}`, testcase);
    }

    function initTestCase() {
        create_backend()
    }

    function cleanupTestCase() {
        testcase.backend.close()
    }
}