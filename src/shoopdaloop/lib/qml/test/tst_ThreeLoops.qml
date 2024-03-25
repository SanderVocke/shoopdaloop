import QtQuick 6.3
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
        initial_descriptor: {
            let track = GenerateSession.generate_default_track(
                "tut",
                2,
                "tut",
                false,
                "tut"
            )
            let _session = GenerateSession.generate_default_session(app_metadata.version_string, null, true, 1, 1, [track])
            return _session
        }

        ShoopSessionTestCase {
            id: testcase
            name: 'ThreeLoops'
            filename : TestFilename.test_filename()
            session: session

            function sync_loop() {
                return session.sync_track.loops[0]
            }

            function first_loop() {
                return session.main_tracks[0].loops[0]
            }

            function second_loop() {
                return session.main_tracks[0].loops[1]
            }

            function clear() {
                sync_loop().clear()
                first_loop().clear()
                second_loop().clear()
                testcase.wait_updated(session.backend)
                verify_loop_cleared(sync_loop())
                verify_loop_cleared(first_loop())
                verify_loop_cleared(second_loop())
            }

            test_fns: ({
                'test_playback_solo_immediate': () => {
                    check_backend()
                    clear()
                    testcase.wait_updated(session.backend)

                    session.backend.dummy_enter_controlled_mode()
                    testcase.wait_controlled_mode(session.backend)
                    testcase.wait_updated(session.backend)

                    first_loop().create_backend_loop()
                    second_loop().create_backend_loop()
                    first_loop().set_length(100)
                    second_loop().set_length(100)
                    testcase.wait_updated(session.backend)

                    // Start playback on both loops
                    first_loop().transition(ShoopConstants.LoopMode.Playing, 0, false)
                    second_loop().transition(ShoopConstants.LoopMode.Playing, 0, false)

                    testcase.wait_updated(session.backend)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Playing)

                    session.backend.dummy_request_controlled_frames(50)
                    session.backend.wait_process()

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Playing)

                    first_loop().play_solo_in_track(0, false)

                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Stopped)
                },
            })
        }
    }
}