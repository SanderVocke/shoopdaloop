import QtQuick 6.6
import QtTest 1.0
import ShoopDaLoop.PythonBackend

import './testDeepEqual.js' as TestDeepEqual
import ShoopConstants
import '../../generate_session.js' as GenerateSession
import './testfilename.js' as TestFilename
import '..'


ShoopTestFile {
    Session {
        id: session
        anchors.fill: parent
        initial_descriptor: GenerateSession.generate_default_session(app_metadata.version_string, null, true)


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
                    // sample_rate not stored in the descriptor yet
                    reference['sample_rate'] = 48000

                    var actual = session.actual_session_descriptor(false, '', null)
                    reference['track_groups'][0]['tracks'][0]['width'] = actual['track_groups'][0]['tracks'][0]['width']
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
}