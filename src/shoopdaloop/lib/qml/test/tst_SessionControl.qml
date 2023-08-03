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
        name: 'SessionControl'
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

        function test_select_multiple_by_functor() {
            start_test_fn('test_select_multiple_by_functor')
            check_backend()
            clear()

            verify_eq(
                session.control_handler.select_loops((l) => { return l == master_loop() }),
                [master_loop().control_handler]
            )
            verify_eq(
                session.control_handler.select_loops((l) => { return l == other_loop() }),
                [other_loop().control_handler]
            )
            verify_eq(
                session.control_handler.select_loops((l) => { return true }),
                [master_loop().control_handler, other_loop().control_handler]
            )

            end_test_fn('test_select_multiple_by_functor')
        }
    }
}