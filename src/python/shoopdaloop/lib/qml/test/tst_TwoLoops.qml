import QtQuick 6.6
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
                1,
                "tut",
                false,
                "tut"
            )
            let _session = GenerateSession.generate_default_session(app_metadata.version_string, null, true, 1, 1, [track])
            return _session
        }

        ShoopSessionTestCase {
            id: testcase
            name: 'TwoLoops'
            filename : TestFilename.test_filename()
            session: session

            function sync_loop() {
                return session.sync_track.loops[0]
            }

            function other_loop() {
                return session.main_tracks[0].loops[0]
            }

            function clear() {
                sync_loop().clear()
                other_loop().clear()
                testcase.wait_updated(session.backend)
                verify_loop_cleared(sync_loop())
                verify_loop_cleared(other_loop())
                registries.state_registry.reset()
            }

            test_fns: ({
                'test_two_loops_cleared': () => {
                    check_backend()
                    clear()
                },

                'test_two_loops_sync_record': () => {
                    check_backend()
                    clear()
                    testcase.wait_updated(session.backend)

                    sync_loop().transition(ShoopConstants.LoopMode.Recording, 0, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    verify_eq(sync_loop().mode, ShoopConstants.LoopMode.Recording)
                    verify_gt(sync_loop().length, 0)
                    verify_loop_cleared(other_loop())

                    clear()
                },

                'test_two_loops_sync_playback': () => {
                    check_backend()

                    sync_loop().set_length(48000)
                    sync_loop().transition(ShoopConstants.LoopMode.Playing, 0, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    verify_eq(sync_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(sync_loop().length, 48000)
                    verify_loop_cleared(other_loop())

                    clear()
                },

                'test_two_loops_countdown': () => {
                    clear()
                    check_backend()

                    session.backend.dummy_enter_controlled_mode()
                    testcase.wait_controlled_mode(session.backend)

                    sync_loop().set_length(100)

                    other_loop().create_backend_loop()
                    other_loop().set_length(100)

                    other_loop().transition(ShoopConstants.LoopMode.Playing, 2, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    sync_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(50)

                    testcase.wait_updated(session.backend)
                    verify_eq(sync_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(other_loop().next_transition_delay, 2)

                    for(var i=0; i<6; i++) {
                        session.backend.dummy_request_controlled_frames(50)
                        session.backend.dummy_run_requested_frames()
                        testcase.wait_updated(session.backend)
                    }

                    testcase.wait_updated(session.backend)
                    verify_eq(sync_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(other_loop().mode, ShoopConstants.LoopMode.Playing)

                    session.backend.dummy_enter_automatic_mode()
                    testcase.wait_updated(session.backend)
                },

                'test_switch_between_backend_and_composite': () => {
                    clear()
                    testcase.wait_updated(session.backend)
                    verify_true(sync_loop().maybe_backend_loop)
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

                    // sync is never composite
                    sync_loop().clear()
                    testcase.wait_updated(session.backend)
                    verify_true(sync_loop().maybe_backend_loop)
                    verify_throw(() => sync_loop.create_composite_loop())
                    verify_true(!sync_loop().maybe_composite_loop)
                    verify_true(!('composition' in sync_loop().actual_session_descriptor()))
                    verify_true('channels' in sync_loop().actual_session_descriptor())

                    other_loop().clear()
                    testcase.wait_updated(session.backend)
                    verify_true(!other_loop().maybe_composite_loop)
                    verify_true(!('composition' in other_loop().actual_session_descriptor()))
                    verify_true('channels' in other_loop().actual_session_descriptor())
                    verify_true(other_loop().actual_session_descriptor().channels.every((channel) => channel.data_length == 0))
                },

                'test_set_gains_then_make_backend_loop': () => {
                    clear()

                    verify_true(!other_loop().maybe_backend_loop)
                    other_loop().push_gain(0.5)
                    other_loop().push_stereo_balance(1.0)
                    testcase.wait_updated(session.backend)
                    verify_approx(other_loop().last_pushed_gain, 0.5)
                    verify_approx(other_loop().last_pushed_stereo_balance, 1.0)

                    other_loop().create_backend_loop()
                    testcase.wait_updated(session.backend)
                    verify_approx(other_loop().last_pushed_gain, 0.5)
                    verify_approx(other_loop().last_pushed_stereo_balance, 1.0)
                    verify_approx(other_loop().audio_channels[0].gain, 0.0)
                    verify_approx(other_loop().audio_channels[1].gain, 0.5)
                }
            })
        }
    }
}