import QtQuick 6.6
import QtTest 1.0
import ShoopDaLoop.PythonBackend

import './testDeepEqual.js' as TestDeepEqual
import ShoopConstants
import '../js/generate_session.js' as GenerateSession
import './testfilename.js' as TestFilename
import '..'

ShoopTestFile {
    Session {
        id: session

        anchors.fill: parent
        initial_descriptor: {
            let track = GenerateSession.generate_default_track(
                "tut",
                3,
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

            function get_loop(idx) {
                return session.main_tracks[0].loops[idx]
            }

            function track_widget() {
                return registries.objects_registry.get(get_loop(0).track_obj_id)
            }

            function clear() {
                testcase.wait_updated(session.backend)
                registries.state_registry.reset()
            }

            test_fns: ({
                'test_reorder_twice': () => {
                    clear()
                    testcase.wait_updated(session.backend)

                    function get_loop_ids() {
                        return [0, 1, 2].map(idx => get_loop(idx).obj_id)
                    }

                    let ori_loops = get_loop_ids()

                    track_widget().move_loop(get_loop(0), 3)
                    testcase.wait_updated(session.backend)
                    verify_eq(get_loop_ids(),
                        [ori_loops[1], ori_loops[2], ori_loops[0]])

                    track_widget().move_loop(get_loop(1), 0)
                    testcase.wait_updated(session.backend)
                    verify_eq(get_loop_ids(),
                        [ori_loops[2], ori_loops[1], ori_loops[0]])
                },
            })
        }
    }
}