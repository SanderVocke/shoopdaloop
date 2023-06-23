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
    update_interval_ms: 10
    client_name_hint: 'ShoopDaLoop'
    backend_type: Types.BackendType.Dummy

    Session {
        id: session
        anchors.fill: parent
        initial_descriptor: GenerateSession.generate_default_session(app_metadata.version_string, 2)

        property var logger : Logger { name: "Test.Control" }

        ShoopSessionTestCase {
            id: testcase
            name: 'TwoLoops'
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

            function test_select_single_by_coords() {
                start_test_fn('test_select_single_by_coords')
                check_backend()
                clear()

                verify_eq(
                    session.control_handler.select_single_loop([0,0]),
                    master_loop().control_handler
                )
                verify_eq(
                    session.control_handler.select_single_loop([0,1]),
                    other_loop().control_handler
                )

                end_test_fn('test_select_single_by_coords')
            }

            function test_select_multiple_by_coords() {
                start_test_fn('test_select_multiple_by_coords')
                check_backend()
                clear()

                verify_eq(
                    session.control_handler.select_loops([[0,0]]),
                    [master_loop().control_handler]
                )
                verify_eq(
                    session.control_handler.select_loops([[0,1]]),
                    [other_loop().control_handler]
                )
                verify_eq(
                    session.control_handler.select_loops([[0,1],[0,0]]),
                    [other_loop().control_handler, master_loop().control_handler]
                )

                end_test_fn('test_select_multiple_by_coords')
            }
        }
    }
}