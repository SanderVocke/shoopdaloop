import QtQuick 6.3
import QtTest 1.0
import ShoopDaLoop.PythonBackend

import './testDeepEqual.js' as TestDeepEqual
import ShoopConstants
import '../../generate_session.js' as GenerateSession
import './testfilename.js' as TestFilename
import '..'

Session {
    id: session

    anchors.fill: parent
    initial_descriptor: GenerateSession.generate_default_session(app_metadata.version_string, null, 2)

    ShoopSessionTestCase {
        id: testcase
        name: 'TwoLoops'
        filename : TestFilename.test_filename()
        session: session

        function master_loop() {
            return session.tracks[0].loops[0]
        }

        function other_loop() {
            return session.tracks[0].loops[1]
        }

        function clear() {
            master_loop().clear()
            other_loop().clear()
            testcase.wait_updated(session.backend)
            verify_loop_cleared(master_loop())
            verify_loop_cleared(other_loop())
        }

        test_fns: ({
            'test_two_loops_cleared': () => {
                check_backend()
                clear()
            },

            'test_two_loops_master_record': () => {
                check_backend()
                clear()
                testcase.wait_updated(session.backend)

                master_loop().transition(ShoopConstants.LoopMode.Recording, 0, true)
                testcase.wait_updated(session.backend)
                verify_eq(master_loop().mode, ShoopConstants.LoopMode.Recording)
                verify_gt(master_loop().length, 0)
                verify_loop_cleared(other_loop())

                clear()
            },

            'test_two_loops_master_playback': () => {
                check_backend()

                master_loop().set_length(48000)
                master_loop().transition(ShoopConstants.LoopMode.Playing, 0, true)
                testcase.wait_updated(session.backend)
                verify_eq(master_loop().mode, ShoopConstants.LoopMode.Playing)
                verify_eq(master_loop().length, 48000)
                verify_loop_cleared(other_loop())

                clear()
            },

            'test_switch_between_backend_and_composite': () => {
                clear()
                testcase.wait_updated(session.backend)
                verify_true(master_loop().maybe_backend_loop)
                other_loop().create_backend_loop()
                verify_true(other_loop().maybe_backend_loop)

                other_loop().clear()
                testcase.wait_updated(session.backend)
                other_loop().create_composite_loop()
                verify_true(!other_loop().maybe_backend_loop)
                verify_true(other_loop().maybe_composite_loop)
                verify_true('composition' in other_loop().actual_session_descriptor())
                verify_true('channels' in other_loop().actual_session_descriptor())
                verify_true(other_loop().actual_session_descriptor().channels.every((channel) => channel.data_length == 0))

                // master is never composite
                master_loop().clear()
                testcase.wait_updated(session.backend)
                verify_true(master_loop().maybe_backend_loop)
                verify_throw(() => master_loop.create_composite_loop())
                verify_true(!master_loop().maybe_composite_loop)
                verify_true(!('composition' in master_loop().actual_session_descriptor()))
                verify_true('channels' in master_loop().actual_session_descriptor())

                other_loop().clear()
                testcase.wait_updated(session.backend)
                verify_true(!other_loop().maybe_composite_loop)
                verify_true(!('composition' in other_loop().actual_session_descriptor()))
                verify_true('channels' in other_loop().actual_session_descriptor())
                verify_true(other_loop().actual_session_descriptor().channels.every((channel) => channel.data_length == 0))
            },
        })
    }
}