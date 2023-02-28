import QtQuick 2.3
import QtTest 1.0


import '../../../build/types.js' as Types

TestCase {
    id: testcase
    name: "Backend"

    function test_click() {
        var qml = 
`import Backend;

Backend {
    update_interval_ms: 30
    client_name_hint: 'ShoopDaLoop'
    backend_type: ${Types.BackendType.Dummy}
}
`
        var item = createTemporaryQmlObject(qml, testcase);
    }
}