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
    initial_descriptor: GenerateSession.generate_default_session(
        app_metadata.version_string,
        null,
        4
        )

    ShoopSessionTestCase {
        id: testcase
        name: 'CompositeLoop_running'
        filename : TestFilename.test_filename()
        session: session

        // master loop
        function m() {
            return session.tracks[0].loops[0]
        }

        // generic loop 1
        function l1() {
            return session.tracks[0].loops[1]
        }

        // generic loop 2
        function l2() {
            return session.tracks[0].loops[2]
        }

        // composite loop
        function c() {
            return session.tracks[0].loops[3]
        }

        function clear() {
            m().clear()
            l1().clear()
            l2().clear()
            c().clear()
            testcase.wait_updated(session.backend)
            verify_loop_cleared(m())
            verify_loop_cleared(l1())
            verify_loop_cleared(l2())
            verify_loop_cleared(c())
        }

        function verify_states(
            m_mode, l1_mode, l2_mode, c_mode,
            m_pos,  l1_pos,  l2_pos,  c_pos
        ) {
            verify_eq(m().mode, m_mode)
            verify_eq(l1().mode, l1_mode)
            verify_eq(l2().mode, l2_mode)
            verify_eq(c().mode, c_mode)
            verify_eq(m().position, m_pos)
            verify_eq(l1().position, l1_pos)
            verify_eq(l2().position, l2_pos)
            verify_eq(c().position, c_pos)
        }

        function process(amount, steps=2) {
            // Note: loop cycle detection will fail if we wrap around exactly
            // (because the loop will have the same position exactly).
            // TODO: a robust fix for this.
            // For now, we work around this by processing in multiple steps.
            for (var i = 0; i < steps; i++) {
                session.backend.dummy_request_controlled_frames(Math.round(amount / steps))
                testcase.wait_updated(session.backend)
            }
        }

        testcase_init_fn: () =>  {
            session.backend.dummy_enter_controlled_mode()
            testcase.wait_updated(session.backend)
        }

        test_fns: ({
            'test_sequential': () => {
                check_backend()
                clear()

                m().set_length(100)

                l1().create_backend_loop()
                l2().create_backend_loop()
                m().create_backend_loop()

                testcase.wait_updated(session.backend)

                l1().set_length(200)
                l2().set_length(300)

                c().create_composite_loop({
                    'playlists': [
                        [ // playlist
                            { 'loop_id': l1().obj_id, 'delay': 0 },
                            { 'loop_id': l2().obj_id, 'delay': 1 },
                        ]
                    ]
                })

                testcase.wait_updated(session.backend)

                verify_states(ShoopConstants.LoopMode.Stopped,
                              ShoopConstants.LoopMode.Stopped,
                              ShoopConstants.LoopMode.Stopped,
                              ShoopConstants.LoopMode.Stopped,
                              0, 0, 0, 0)

                m().transition(ShoopConstants.LoopMode.Playing, 0, false)
                c().transition(ShoopConstants.LoopMode.Playing, 0, false)

                process(50) // middle of 1st step

                verify_states(ShoopConstants.LoopMode.Playing,
                              ShoopConstants.LoopMode.Playing,
                              ShoopConstants.LoopMode.Stopped,
                              ShoopConstants.LoopMode.Playing,
                              50, 50, 0, 50)

                process(100) // middle of 2nd step

                verify_states(ShoopConstants.LoopMode.Playing,
                              ShoopConstants.LoopMode.Playing,
                              ShoopConstants.LoopMode.Stopped,
                              ShoopConstants.LoopMode.Playing,
                              50, 150, 0, 150)

                process(100) // middle of 3rd step

                verify_states(ShoopConstants.LoopMode.Playing,
                              ShoopConstants.LoopMode.Stopped,
                              ShoopConstants.LoopMode.Stopped,
                              ShoopConstants.LoopMode.Playing,
                              50, 0, 0, 250)

                process(100) // middle of 4th step

                verify_states(ShoopConstants.LoopMode.Playing,
                              ShoopConstants.LoopMode.Stopped,
                              ShoopConstants.LoopMode.Playing,
                              ShoopConstants.LoopMode.Playing,
                              50, 0, 50, 350)

                process(200, 4) // middle of 6th step

                verify_states(ShoopConstants.LoopMode.Playing,
                              ShoopConstants.LoopMode.Stopped,
                              ShoopConstants.LoopMode.Playing,
                              ShoopConstants.LoopMode.Playing,
                              50, 0, 250, 550)

                process(100) // middle of 1st step after looping around

                verify_states(ShoopConstants.LoopMode.Playing,
                              ShoopConstants.LoopMode.Playing,
                              ShoopConstants.LoopMode.Stopped,
                              ShoopConstants.LoopMode.Playing,
                              50, 50, 0, 50)
            },
        })
    }
}