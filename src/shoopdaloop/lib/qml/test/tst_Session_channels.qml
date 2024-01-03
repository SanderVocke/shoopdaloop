import QtQuick 6.3
import QtTest 1.0
import ShoopDaLoop.PythonBackend

import './testDeepEqual.js' as TestDeepEqual
import ShoopConstants
import '../../generate_session.js' as GenerateSession
import './testfilename.js' as TestFilename
import '..'

// Note: regression test for https://github.com/SanderVocke/shoopdaloop/issues/77

ShoopTestFile {
    Session {
        id: session

        anchors.fill: parent
        initial_descriptor: {
            let base = GenerateSession.generate_default_session(app_metadata.version_string, null, true, 1)
            let direct_track = GenerateSession.generate_default_track(
                "dt",
                1,
                "dt",
                false,
                "dt",
                0,
                0,
                2,
                false,
                false,
                false,
                undefined
                )
            base.tracks.push(direct_track)
            testcase.logger.debug(() => ("session descriptor: " + JSON.stringify(base, null, 2)))
            return base
        }

        ShoopSessionTestCase {
            id: testcase
            name: 'Session_channels'
            filename : TestFilename.test_filename()
            session: session

            function track() { return session.tracks[1] }

            function loop() { return track().loops[0] }

            testcase_init_fn: () =>  {
                run_case("initTestCase" , () => {
                    session.backend.dummy_enter_controlled_mode()
                })
            }

            test_fns: ({
                "test_channels": () => {
                    check_backend()
                    let ori = session.initial_descriptor
                    // sample_rate is not there in the beginning, but will be after save+load
                    ori['sample_rate'] = 48000
                    for(var i=0; i<ori['tracks'].length; i++) {
                        ori['tracks'][i]['width'] = session.actual_session_descriptor()['tracks'][i]['width']
                    }

                    verify_true(loop())
                    verify_true('channels' in session.initial_descriptor.tracks[0].loops[0])
                    verify_true('channels' in loop().actual_session_descriptor())
                    verify_eq(loop().actual_session_descriptor().channels.length, 2)
                    testcase.wait_updated(session.backend)

                    var filename = file_io.generate_temporary_filename() + '.shl'
                    session.save_session(filename)

                    testcase.wait_session_io_done()
                    testcase.wait_updated(session.backend)

                    session.load_session(filename)
                    testcase.wait_session_loaded(session)

                    verify_true(loop())
                    verify_eq(loop().actual_session_descriptor().channels.length, 2)

                    var actual = session.actual_session_descriptor()
                    verify_eq(actual, ori)
                }
            })
        }
    }
}