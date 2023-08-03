import QtQuick 6.3
import QtTest 1.0
import ShoopDaLoop.PythonBackend

import './testDeepEqual.js' as TestDeepEqual
import '../../generated/types.js' as Types
import '../../generate_session.js' as GenerateSession
import './testfilename.js' as TestFilename
import '..'

PythonBackend {
    id: backend
    update_interval_ms: 30
    client_name_hint: 'ShoopDaLoop'
    backend_type: Types.BackendType.Dummy

    Session {
        id: session
        anchors.fill: parent
        initial_descriptor: GenerateSession.generate_default_session(app_metadata.version_string)


        ShoopSessionTestCase {
            id: testcase
            name: 'SessionDescriptor_default'
            filename : TestFilename.test_filename()
            session: session
            backend: backend

            function test_session_descriptor_default() {
                start_test_fn('test_session_descriptor_default')
                check_backend()

                testcase.wait(500)
                var reference = session.initial_descriptor
                var actual = session.actual_session_descriptor(false, '', null)
                verify(TestDeepEqual.testDeepEqual(actual, reference, session.logger.error))

                var filename = file_io.generate_temporary_filename() + '.shl'

                try {
                    session.logger.info("Saving session to " + filename)
                    session.save_session(filename)

                    testcase.wait(500)

                    session.logger.info("Re-loading session")
                    session.load_session(filename)

                    testcase.wait(500)
                    
                    actual = session.actual_session_descriptor(false, '', null)
                    verify(TestDeepEqual.testDeepEqual(actual, reference, session.logger.error))
                } finally {
                    file_io.delete_file(filename)
                }

                end_test_fn('test_session_descriptor_default');
            }
        }
    }
}