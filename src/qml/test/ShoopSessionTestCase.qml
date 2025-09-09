import QtQuick 6.6

ShoopTestCase {
    id: root
    property var session : null
    property var backend : session ? session.backend : null
    property bool additional_when_condition: true
    property bool total_precondition: session && session.loaded && additional_when_condition

    onTotal_preconditionChanged: {
        if (total_precondition) {
            timer.start()
        }
    }

    Timer {
        id: timer
        interval: 100
        repeat: false
        property bool done : false
        onTriggered: done = true
    }

    when: timer.done

    function check_backend() {
        verify(backend && backend.ready, "backend not initialized")
        wait_updated(backend)
    }

    testcase_init_fn: () =>  {
        check_backend()
    }

    testcase_deinit_fn: () => {
        backend.close()
    }
}