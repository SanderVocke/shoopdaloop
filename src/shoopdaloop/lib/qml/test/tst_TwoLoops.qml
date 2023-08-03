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
    update_interval_ms: 10
    client_name_hint: 'ShoopDaLoop'
    backend_type: Types.BackendType.Dummy

    Session {
        id: session
        anchors.fill: parent
        initial_descriptor: GenerateSession.generate_default_session(app_metadata.version_string, 2)

        ShoopSessionTestCase {
            id: testcase
            name: 'TwoLoops'
            filename : TestFilename.test_filename()
            session: session
            backend: backend

            function master_loop() {
                return session.tracks[0].loops[0]
            }

            function other_loop() {
                return session.tracks[0].loops[1]
            }

            function clear() {
                master_loop().clear()
                other_loop().clear()
                testcase.wait(100)
                verify_loop_cleared(master_loop())
                verify_loop_cleared(other_loop())
            }

            function test_two_loops_cleared() {
                start_test_fn('test_two_loops_cleared')
                check_backend()

                clear()

                end_test_fn('test_two_loops_cleared')
            }

            function test_two_loops_master_record() {
                start_test_fn('test_two_loops_master_record')
                check_backend()

                master_loop().transition(Types.LoopMode.Recording, 0, true)
                testcase.wait(100)
                verify_eq(master_loop().mode, Types.LoopMode.Recording)
                verify_gt(master_loop().length, 0)
                verify_loop_cleared(other_loop())

                clear()
                end_test_fn('test_two_loops_master_record')
            }

            function test_two_loops_master_playback() {
                start_test_fn('test_two_loops_master_playback')
                check_backend()

                master_loop().set_length(48000)
                master_loop().transition(Types.LoopMode.Playing, 0, true)
                testcase.wait(100)
                verify_eq(master_loop().mode, Types.LoopMode.Playing)
                verify_eq(master_loop().length, 48000)
                verify_loop_cleared(other_loop())

                clear()
                end_test_fn('test_two_loops_master_playback')
            }
        }
    }
}