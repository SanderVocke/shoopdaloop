import QtQuick 2.3
import QtTest 1.0

TestCase {
    TestCase {
    id: testcase
    name: "Backend"

    function test_click() {
        var qml = 
`import Backend;

Backend {
    update_interval_ms: 30
    client_name_hint: 'ShoopDaLoop'
}
`
        var item = createTemporaryQmlObject(qml, testcase);
        var audiosys = item.maybe_backend_test_audio_system()
        console.log(audiosys)
    }
}
}