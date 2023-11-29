import QtQuick 6.3
import QtTest 1.0
import ShoopDaLoop.PythonBackend

import './testDeepEqual.js' as TestDeepEqual
import '../../generated/types.js' as Types
import '../../generate_session.js' as GenerateSession
import './testfilename.js' as TestFilename
import '..'


Session {
    id: session
    anchors.fill: parent
    initial_descriptor: GenerateSession.generate_default_session(app_metadata.version_string)


    ShoopSessionTestCase {
        id: testcase
        name: 'SessionDescriptor_default'
        filename : TestFilename.test_filename()
        session: session

        test_fns: ({
            'test_session_descriptor_default': () => {
                check_backend()

                testcase.wait_session_loaded(session)
                var reference = session.initial_descriptor
                var actual = session.actual_session_descriptor(false, '', null)
                verify(TestDeepEqual.testDeepEqual(actual, reference, session.logger.error))

                var filename = file_io.generate_temporary_filename() + '.shl'

                session.logger.info(() => ("Saving session to " + filename))
                session.save_session(filename)
                
                testcase.wait_session_io_done()

                session.logger.info(() => ("Re-loading session"))
                session.load_session(filename)

                testcase.wait_session_io_done()
                testcase.wait_session_loaded(session)
                    
                actual = session.actual_session_descriptor(false, '', null)

                file_io.delete_file(filename)

                verify(TestDeepEqual.testDeepEqual(actual, reference, session.logger.error))
            }
        })
    }
}