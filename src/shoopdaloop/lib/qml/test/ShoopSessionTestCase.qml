import QtQuick 6.3
import QtTest 1.0

ShoopTestCase {
    id: root
    property var session : null
    property var backend : session ? session.backend : null
    property bool additional_when_condition: true

    Timer {
        id: timer
        running: session && session.loaded && additional_when_condition
        interval: 100
        repeat: false
        property bool done : false
        onTriggered: done = true
    }

    when: timer.done

    function check_backend() {
        verify(backend && backend.initialized, "backend not initialized")
        backend.doUpdate()
    }

    testcase_init_fn: () =>  {
        check_backend()
    }
}