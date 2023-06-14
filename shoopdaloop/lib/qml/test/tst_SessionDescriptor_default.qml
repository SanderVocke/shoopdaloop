import QtQuick 6.3
import QtTest 1.0
import Backend
import Logger

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
        initial_descriptor: GenerateSession.generate_default_session(app_metadata.version_string)

        property var logger : Logger { name: "Test" }

        TestCase {
            id: testcase

            Timer {
                id: timer
                running: session.loaded
                interval: 100
                repeat: false
                property bool done : false
                onTriggered: done = true
            }

            when: timer.done

            Timer {
                id: delayer
            }

            function test_session_descriptor_default() {
                verify(backend.initialized, "backend not initialized")
                backend.doUpdate()

                var reference = session.initial_descriptor
                var actual = session.actual_session_descriptor(false, '', null)
                verify(TestDeepEqual.testDeepEqual(reference, actual, session.logger.error))

                var filename = file_io.generate_temporary_filename() + '.shl'

                try {
                    session.logger.info("Saving session to " + filename)
                    session.save_session(filename)

                    testcase.wait(500)

                    session.logger.info("Re-loading session")
                    session.load_session(filename)

                    testcase.wait(500)
                    
                    actual = session.actual_session_descriptor(false, '', null)
                    verify(TestDeepEqual.testDeepEqual(reference, actual, session.logger.error))
                } finally {
                    file_io.delete_file(filename)
                }
            }

            function cleanupTestCase() { backend.close() }
        }
    }
}