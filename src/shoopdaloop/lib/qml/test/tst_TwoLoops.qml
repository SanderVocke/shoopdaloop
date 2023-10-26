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
    initial_descriptor: GenerateSession.generate_default_session(app_metadata.version_string, 2)

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

        function test_two_loops_cleared() {
            run_case('test_two_loops_cleared', () => {
                check_backend()
                clear()
            })
        }

        function test_two_loops_master_record() {
            run_case('test_two_loops_master_record', () => {
                check_backend()
                clear()

                master_loop().transition(Types.LoopMode.Recording, 0, true)
                testcase.wait_updated(session.backend)
                verify_eq(master_loop().mode, Types.LoopMode.Recording)
                verify_gt(master_loop().length, 0)
                verify_loop_cleared(other_loop())

                clear()
            })
        }

        function test_two_loops_master_playback() {
            run_case('test_two_loops_master_playback', () => {
                check_backend()

                master_loop().set_length(48000)
                master_loop().transition(Types.LoopMode.Playing, 0, true)
                testcase.wait_updated(session.backend)
                verify_eq(master_loop().mode, Types.LoopMode.Playing)
                verify_eq(master_loop().length, 48000)
                verify_loop_cleared(other_loop())

                clear()
            })
        }

        function test_switch_between_backend_and_composite() {
            run_case('test_switch_between_backend_and_composite', () => {
                clear()
                verify_true(master_loop().maybe_backend_loop)
                other_loop().create_backend_loop()
                verify_true(other_loop().maybe_backend_loop)

                other_loop().clear()
                other_loop().create_composite_loop()
                verify_true(!other_loop().maybe_backend_loop)
                verify_true(other_loop().maybe_composite_loop)
                verify_true('composition' in other_loop().actual_session_descriptor())
                verify_true(!('channels' in other_loop().actual_session_descriptor()))

                // master is never composite
                master_loop().clear()
                verify_true(master_loop().maybe_backend_loop)
                verify_throw(() => master_loop.create_composite_loop())
                verify_true(!master_loop().maybe_composite_loop)
                verify_true(!('composition' in master_loop().actual_session_descriptor()))
                verify_true('channels' in master_loop().actual_session_descriptor())

                other_loop().clear()
                verify_true(!other_loop().maybe_composite_loop)
                verify_true(!('composition' in other_loop().actual_session_descriptor()))
                verify_true(!('channels' in other_loop().actual_session_descriptor()))
            })
        }
    }
}