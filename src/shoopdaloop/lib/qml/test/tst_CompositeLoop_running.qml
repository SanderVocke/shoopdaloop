import QtQuick 6.3
import QtTest 1.0
import ShoopDaLoop.PythonBackend
import ShoopDaLoop.PythonDummyProcessHelper

import './testDeepEqual.js' as TestDeepEqual
import ShoopConstants
import '../../generate_session.js' as GenerateSession
import './testfilename.js' as TestFilename
import '../../delay.js' as Delay
import '..'

ShoopTestFile {
    Session {
        id: session

        anchors.fill: parent
        initial_descriptor: {
            let track = GenerateSession.generate_default_track(
                'track1',
                3,
                'track1',
                false,
                'track1'
            )
            return GenerateSession.generate_default_session(
                app_metadata.version_string,
                null,
                true, 1, 1, [track]
            )
        }

        ShoopSessionTestCase {
            id: testcase
            name: 'CompositeLoop_running'
            filename : TestFilename.test_filename()
            session: session

            // sync loop
            function m() {
                return session.sync_track.loops[0]
            }

            // generic loop 1
            function l1() {
                return session.main_tracks[0].loops[0]
            }

            // generic loop 2
            function l2() {
                return session.main_tracks[0].loops[1]
            }

            // composite loop
            function c() {
                return session.main_tracks[0].loops[2]
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
                m_mode=undefined,
                l1_mode=undefined,
                l2_mode=undefined,
                c_mode=undefined,
                m_pos=undefined,
                l1_pos=undefined,
                l2_pos=undefined,
                c_pos=undefined,
                m_length=undefined,
                l1_length=undefined,
                l2_length=undefined,
                c_length=undefined
            ) {
                if(m_mode !== undefined) { verify_eq(m().mode, m_mode, 'sync loop mode') }
                if(l1_mode !== undefined) { verify_eq(l1().mode, l1_mode, 'loop 1 mode') }
                if(l2_mode !== undefined) { verify_eq(l2().mode, l2_mode, 'loop 2 mode') }
                if(c_mode !== undefined) { verify_eq(c().mode, c_mode, 'composite loop mode') }
                if(m_pos !== undefined) { verify_eq(m().position, m_pos, 'sync loop pos') }
                if(l1_pos !== undefined) { verify_eq(l1().position, l1_pos, 'loop 1 pos') }
                if(l2_pos !== undefined) { verify_eq(l2().position, l2_pos, 'loop 2 pos') }
                if(c_pos !== undefined) { verify_eq(c().position, c_pos, 'composite loop pos') }
                if(m_length !== undefined) { verify_eq(m().length, m_length, 'sync loop length') }
                if(l1_length !== undefined) { verify_eq(l1().length, l1_length, 'loop 1 length') }
                if(l2_length !== undefined) { verify_eq(l2().length, l2_length, 'loop 2 length') }
                if(c_length !== undefined) { verify_eq(c().length, c_length, 'composite loop length') }
            }

            function process(amount, steps=2) {
                // Note: loop cycle detection will fail if we wrap around exactly
                // (because the loop will have the same position exactly).
                // TODO: a robust fix for this.
                // For now, we work around this by processing in multiple steps.
                // This is also realistic in terms of how the application is normally used:
                // small process amounts with larger loops.
                for (var i = 0; i < steps; i++) {
                    session.backend.dummy_request_controlled_frames(Math.round(amount / steps))
                    testcase.wait_updated(session.backend)
                }
            }

            function start_process_async(n_frames) {
                process_helper.wait_interval = 0.01
                process_helper.wait_start = 0.02
                process_helper.samples_per_iter = session.backend.get_sample_rate() * process_helper.wait_interval
                process_helper.n_iters = n_frames / process_helper.samples_per_iter
                console.log(process_helper.samples_per_iter, n_frames, process_helper.n_iters)
                process_helper.start()
            }

            PythonDummyProcessHelper {
                id: process_helper
                backend: session.backend

                function wait() {
                    while(active) {
                        Delay.blocking_delay(100)
                    }
                }
                property real total_duration : wait_start + n_iters * wait_interval
            }

            testcase_init_fn: () =>  {
                session.backend.dummy_enter_controlled_mode()
                testcase.wait_controlled_mode(session.backend)
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
                                [{ 'loop_id': l1().obj_id, 'delay': 0 }],
                                [{ 'loop_id': l2().obj_id, 'delay': 1 }],
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

                    process(50); // sync loop is playing

                    // trigger the composite loop
                    c().transition(ShoopConstants.LoopMode.Playing, 0, true)
                    testcase.wait_updated(session.backend)
                    verify_eq(l1().next_mode, ShoopConstants.LoopMode.Playing)

                    process(100) // middle of 1st step

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


                'test_play_after_record': () => {
                    check_backend()
                    clear()

                    m().set_length(100)

                    testcase.wait_updated(session.backend)

                    c().create_composite_loop({
                        'playlists': [
                            [ // playlist
                                [{ 'loop_id': l1().obj_id, 'delay': 0, 'n_cycles': 2 }],
                                [{ 'loop_id': l2().obj_id, 'delay': 1 }],
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

                    process(50); // sync loop is playing

                    // trigger the composite loop
                    c().transition(ShoopConstants.LoopMode.Recording, 0, true)
                    testcase.wait_updated(session.backend)
                    verify_eq(l1().next_mode, ShoopConstants.LoopMode.Recording)

                    process(100) // middle of 1st step

                    verify_states(ShoopConstants.LoopMode.Playing, // sync
                                ShoopConstants.LoopMode.Recording, // l1
                                ShoopConstants.LoopMode.Stopped,   // l2
                                ShoopConstants.LoopMode.Recording, // c
                                50, 0, 0, 50,
                                100, 50, 0, 400)

                    process(100) // middle of 2nd step

                    verify_states(ShoopConstants.LoopMode.Playing, // sync
                                ShoopConstants.LoopMode.Recording, // l1
                                ShoopConstants.LoopMode.Stopped,   // l2
                                ShoopConstants.LoopMode.Recording, // c
                                50, 0, 0, 150,
                                100, 150, 0, 400)
                    
                    process(100) // middle of 3rd step (delay)

                    verify_states(ShoopConstants.LoopMode.Playing, // sync
                                ShoopConstants.LoopMode.Stopped,   // l1
                                ShoopConstants.LoopMode.Stopped,   // l2
                                ShoopConstants.LoopMode.Recording, // c
                                50, 0, 0, 250,
                                100, 200, 0, 400)
                    
                    process(100) // middle of 4th step (record 2nd loop)

                    verify_states(ShoopConstants.LoopMode.Playing, // sync
                                ShoopConstants.LoopMode.Stopped,   // l1
                                ShoopConstants.LoopMode.Recording,   // l2
                                ShoopConstants.LoopMode.Recording, // c
                                50, 0, 0, 350,
                                100, 200, 50, 400)

                    process(100) // middle of 5th step (whole thing starts playing after finish recording)

                    verify_states(ShoopConstants.LoopMode.Playing, // sync
                                ShoopConstants.LoopMode.Playing,   // l1
                                ShoopConstants.LoopMode.Stopped,   // l2
                                ShoopConstants.LoopMode.Playing, // c
                                50, 50, 0, 50,
                                100, 200, 100, 400)
                },

                'test_parallel_playlist': () => {
                    check_backend()
                    clear()

                    l1().create_backend_loop()
                    l2().create_backend_loop()
                    m().create_backend_loop()

                    testcase.wait_updated(session.backend)

                    m().set_length(100)
                    l1().set_length(200)
                    l2().set_length(100)

                    c().create_composite_loop({
                        'playlists': [
                            [ // playlist
                                [{ 'loop_id': l1().obj_id, 'delay': 0, 'n_cycles': 1 }],
                                [{ 'loop_id': l1().obj_id, 'delay': 1, 'n_cycles': 2 },
                                 { 'loop_id': l2().obj_id, 'delay': 2, 'n_cycles': 1 }],
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

                    process(50); // sync loop is playing

                    // trigger the composite loop
                    c().transition(ShoopConstants.LoopMode.Playing, 0, true)
                    testcase.wait_updated(session.backend)
                    verify_eq(l1().next_mode, ShoopConstants.LoopMode.Playing)

                    process(100) // middle of 1st step (1st loop plays)

                    verify_states(ShoopConstants.LoopMode.Playing,
                                ShoopConstants.LoopMode.Playing,
                                ShoopConstants.LoopMode.Stopped,
                                ShoopConstants.LoopMode.Playing,
                                50, 50, 0, 50,
                                100, 200, 100, 400)

                    process(100) // middle of 2nd step (pause)

                    verify_states(ShoopConstants.LoopMode.Playing,
                                ShoopConstants.LoopMode.Stopped,
                                ShoopConstants.LoopMode.Stopped,
                                ShoopConstants.LoopMode.Playing,
                                50, 0, 0, 150,
                                100, 200, 100, 400)
                    
                    process(100) // middle of 3rd step (1st loop plays from the top)

                    verify_states(ShoopConstants.LoopMode.Playing,
                                ShoopConstants.LoopMode.Playing,
                                ShoopConstants.LoopMode.Stopped,
                                ShoopConstants.LoopMode.Playing,
                                50, 50, 0, 250,
                                100, 200, 100, 400)
                    
                    process(100) // middle of 4th step (1st loop continues playing, 2nd loop starts playing)

                    verify_states(ShoopConstants.LoopMode.Playing,
                                ShoopConstants.LoopMode.Playing,
                                ShoopConstants.LoopMode.Playing,
                                ShoopConstants.LoopMode.Playing,
                                50, 150, 50, 350,
                                100, 200, 100, 400)
                    
                },

                'test_script': () => {
                    check_backend()
                    clear()

                    m().set_length(100)

                    testcase.wait_updated(session.backend)

                    c().create_composite_loop({
                        'playlists': [
                            [ // playlist
                                [{ 'loop_id': l1().obj_id, 'delay': 0, 'n_cycles': 1, 'mode': ShoopConstants.LoopMode.Recording }],
                                [{ 'loop_id': l2().obj_id, 'delay': 0, 'n_cycles': 1, 'mode': ShoopConstants.LoopMode.Recording }],
                                [{ 'loop_id': l1().obj_id, 'delay': 0, 'n_cycles': 1, 'mode': ShoopConstants.LoopMode.Playing }],
                                [{ 'loop_id': l2().obj_id, 'delay': 0, 'n_cycles': 1, 'mode': ShoopConstants.LoopMode.Playing }],
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

                    process(50); // sync loop is playing

                    // trigger the composite loop
                    c().transition(ShoopConstants.LoopMode.Playing, 0, true)
                    testcase.wait_updated(session.backend)
                    verify_eq(l1().next_mode, ShoopConstants.LoopMode.Recording)

                    process(100) // middle of 1st step (record l1)

                    verify_states(ShoopConstants.LoopMode.Playing, // sync
                                ShoopConstants.LoopMode.Recording, // l1
                                ShoopConstants.LoopMode.Stopped,   // l2
                                ShoopConstants.LoopMode.Playing,   // c
                                50, 0, 0, 50,
                                100, 50, 0, 400)

                    process(100) // middle of 2nd step (record l2)

                    verify_states(ShoopConstants.LoopMode.Playing, // sync
                                ShoopConstants.LoopMode.Stopped,   // l1
                                ShoopConstants.LoopMode.Recording, // l2
                                ShoopConstants.LoopMode.Playing,   // c
                                50, 0, 0, 150,
                                100, 100, 50, 400)
                    
                    process(100) // middle of 3rd step (play l1)

                    verify_states(ShoopConstants.LoopMode.Playing, // sync
                                ShoopConstants.LoopMode.Playing,   // l1
                                ShoopConstants.LoopMode.Stopped,   // l2
                                ShoopConstants.LoopMode.Playing,   // c
                                50, 50, 0, 250,
                                100, 100, 100, 400)
                    
                    process(100) // middle of 4th step (play l2)

                    verify_states(ShoopConstants.LoopMode.Playing, // sync
                                ShoopConstants.LoopMode.Stopped,   // l1
                                ShoopConstants.LoopMode.Playing,   // l2
                                ShoopConstants.LoopMode.Playing,   // c
                                50, 0, 50, 350,
                                100, 100, 100, 400)

                    process(100) // middle of 5th step (script ended)

                    verify_states(ShoopConstants.LoopMode.Playing, // sync
                                ShoopConstants.LoopMode.Stopped,   // l1
                                ShoopConstants.LoopMode.Stopped,   // l2
                                ShoopConstants.LoopMode.Stopped, // c
                                50, 0, 0, 0,
                                100, 100, 100, 400)
                },

                'test_countdown': () => {
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
                                [{ 'loop_id': l1().obj_id, 'delay': 0 }],
                                [{ 'loop_id': l2().obj_id, 'delay': 1 }],
                            ]
                        ]
                    })

                    c().transition(ShoopConstants.LoopMode.Playing, 3, true)
                    m().transition(ShoopConstants.LoopMode.Playing, 0, false)

                    testcase.wait_updated(session.backend)

                    verify_eq(c().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(c().next_mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(c().next_transition_delay, 3)

                    process(350, 7) // a bunch of sync loop cycles

                    verify_eq(c().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(c().next_mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(c().next_transition_delay, 0)

                    verify_eq(l1().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(l1().next_mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(l1().next_transition_delay, 0)

                    process(100)

                    verify_eq(c().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(l1().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(c().maybe_loop.iteration, 0)
                },


                'test_ui_frozen': () => {
                    // Tests that when the UI thread freezes / is busy, composite loops
                    // still do their thing in the background.
                    check_backend()
                    clear()

                    m().set_length(12000) // 1/4s
                    l1().create_backend_loop()
                    l2().create_backend_loop()
                    m().create_backend_loop()

                    testcase.wait_updated(session.backend)

                    l1().set_length(12000) // 1/4s
                    l2().set_length(12000)

                    c().create_composite_loop({
                        'playlists': [
                            [ // playlist
                                [{ 'loop_id': l1().obj_id, 'delay': 0 }],
                                [{ 'loop_id': l2().obj_id, 'delay': 1 }],
                            ]
                        ]
                    })

                    c().transition(ShoopConstants.LoopMode.Playing, 0, true)
                    m().transition(ShoopConstants.LoopMode.Playing, 0, false)

                    testcase.wait_updated(session.backend)

                    verify_eq(c().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(c().next_mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(c().next_transition_delay, 0)

                    start_process_async(42000)
                    // We started the process helper to process. Now, freeze the GUI
                    // while the loops continue in the background.
                    let d = 1000 * process_helper.total_duration * 2 // * 2 for overrun
                    Delay.blocking_delay(d)
                    process_helper.wait()

                    testcase.wait_updated(session.backend)
                    verify_eq(c().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(l1().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(l2().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(c().maybe_loop.iteration, 2)
                },
            })
        }
    }
}