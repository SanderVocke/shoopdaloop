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
                registries.state_registry.reset()
            }

            test_fns: ({
                'test_playback_immediate': () => {
                    clear()

                    first_loop().create_backend_loop()
                    first_loop().set_length(48000)
                    registries.state_registry.set_sync_active(false)
                    testcase.wait_updated(session.backend)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)

                    first_loop().on_play_clicked()
                    testcase.wait_updated(session.backend)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                },

                'test_playback_immediate_solo': () => {
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
                    registries.state_registry.set_sync_active(false)
                    first_loop().on_play_clicked()
                    second_loop().on_play_clicked()

                    testcase.wait_updated(session.backend)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Playing)

                    // Now play the first loop solo
                    registries.state_registry.set_solo_active(true)
                    first_loop().on_play_clicked()

                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(second_loop().next_transition_delay, -1) // nothing planned
                },

                'test_playback_synced': () => {
                    verify_true(false)
                },

                'test_playback_synced_solo': () => {
                    verify_true(false)
                },
                
                'test_playback_targeted': () => {
                    verify_true(false)
                },

                'test_playback_targeted_solo': () => {
                    verify_true(false)
                },

                'test_playdry_immediate': () => {
                    clear()

                    first_loop().create_backend_loop()
                    first_loop().set_length(48000)
                    registries.state_registry.set_sync_active(false)
                    testcase.wait_updated(session.backend)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)

                    first_loop().on_playdry_clicked()
                    testcase.wait_updated(session.backend)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.PlayingDryThroughWet)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                },

                'test_playdry_immediate_solo': () => {
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
                    registries.state_registry.set_sync_active(false)
                    first_loop().on_play_clicked()
                    second_loop().on_play_clicked()

                    testcase.wait_updated(session.backend)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Playing)

                    // Now play the first loop solo
                    registries.state_registry.set_solo_active(true)
                    first_loop().on_playdry_clicked()

                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.PlayingDryThroughWet)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(second_loop().next_transition_delay, -1) // nothing planned
                },

                'test_playdry_synced': () => {
                    verify_true(false)
                },

                'test_playdry_synced_solo': () => {
                    verify_true(false)
                },
                
                'test_playdry_targeted': () => {
                    verify_true(false)
                },

                'test_playback_targeted_solo': () => {
                    verify_true(false)
                },

                'test_record_infinite_immediate': () => {
                    clear()

                    first_loop().create_backend_loop()
                    first_loop().set_length(48000)
                    registries.state_registry.set_sync_active(false)
                    testcase.wait_updated(session.backend)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)

                    first_loop().on_record_clicked()
                    testcase.wait_updated(session.backend)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Recording)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                },

                'test_record_infinite_synced': () => {
                    verify_true(false)
                },

                'test_record_2_then_play': () => {
                    verify_true(false)
                },

                'test_record_2_then_stop': () => {
                    verify_true(false)
                },

                'test_record_with_targeted': () => {
                    verify_true(false)
                },

                'test_record_infinite_solo': () => {
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
                    registries.state_registry.set_sync_active(false)
                    first_loop().on_play_clicked()
                    second_loop().on_play_clicked()

                    testcase.wait_updated(session.backend)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Playing)

                    // Now play the first loop solo
                    registries.state_registry.set_solo_active(true)
                    first_loop().on_record_clicked()

                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Recording)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(second_loop().next_transition_delay, -1) // nothing planned
                },

                'test_grab_ringbuffer_no_play': () => {
                    clear()
                    session.backend.dummy_request_controlled_frames(500);
                    session.backend.wait_process()
                    testcase.wait_updated(session.backend)
                    registries.state_registry.set_play_after_record_active(false)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    first_loop().on_grab_clicked()

                    testcase.wait_updated(session.backend)
                    verify_true(first_loop().length > 0)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                },

                'test_grab_ringbuffer_play': () => {
                    clear()
                    session.backend.dummy_request_controlled_frames(500);
                    session.backend.wait_process()
                    testcase.wait_updated(session.backend)
                    registries.state_registry.set_play_after_record_active(true)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    first_loop().on_grab_clicked()

                    testcase.wait_updated(session.backend)
                    verify_true(first_loop().length > 0)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                },

                'test_grab_ringbuffer_play_solo': () => {
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
                    registries.state_registry.set_sync_active(false)
                    first_loop().on_play_clicked()
                    second_loop().on_play_clicked()

                    testcase.wait_updated(session.backend)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Playing)

                    // Now grab and play
                    session.backend.dummy_request_controlled_frames(500);
                    session.backend.wait_process()
                    testcase.wait_updated(session.backend)
                    registries.state_registry.set_play_after_record_active(true)
                    registries.state_registry.set_solo_active(true)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    first_loop().on_grab_clicked()

                    testcase.wait_updated(session.backend)
                    verify_true(first_loop().length > 0)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(second_loop().next_transition_delay, -1) // nothing planned
                },

                'test_stop_immediate': () => {
                    clear()

                    first_loop().create_backend_loop()
                    first_loop().set_length(48000)
                    registries.state_registry.set_sync_active(false)
                    first_loop().transition(ShoopConstants.LoopMode.Recording, 0, false)
                    testcase.wait_updated(session.backend)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Recording)

                    first_loop().on_stop_clicked()
                    testcase.wait_updated(session.backend)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                },

                'test_stop_synced': () => {
                    verify_true(false)
                },

                'test_stop_with_targeted': () => {
                    verify_true(false)
                }
            })
        }
    }
}