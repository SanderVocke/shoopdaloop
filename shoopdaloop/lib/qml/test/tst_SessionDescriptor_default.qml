import QtQuick 2.3
import QtTest 1.0
import Backend

import './testDeepEqual.js' as TestDeepEqual
import '../../backend/frontend_interface/types.js' as Types
import '../../generate_session.js' as GenerateSession
import '..'

Backend {
    id: backend
    update_interval_ms: 30
    client_name_hint: 'ShoopDaLoop'
    backend_type: Types.BackendType.Dummy

    Session {
        id: session
        anchors.fill: parent
        initial_descriptor: GenerateSession.generate_default_session()

        TestCase {
            Timer {
                id: timer
                running: session.loaded
                interval: 100
                repeat: false
                property bool done : false
                onTriggered: done = true
            }

            when: timer.done

            function test_session_descriptor_default() {
                verify(backend.initialized, "backend not initialized")
                backend.doUpdate()

                var reference = session.initial_descriptor
                var actual = session.actual_session_descriptor(false, '', null)
                verify(TestDeepEqual.testDeepEqual(reference, actual))

                var dir = file_io.create_temporary_folder()

                try {
                    session.save_session(dir)
                    console.log("Session saved to", dir)

                    var readback = JSON.parse(file_io.read_file(dir + '/session.json'))
                    verify(TestDeepEqual.testDeepEqual(reference, readback))
                } finally {
                    file_io.delete_recursive(dir)
                }
            }

            function cleanupTestCase() { backend.close() }
        }
    }
}