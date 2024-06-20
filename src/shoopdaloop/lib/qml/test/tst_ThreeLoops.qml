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
                2,
                "tut",
                false,
                "tut"
            )
            let _session = GenerateSession.generate_default_session(app_metadata.version_string, null, true, 1, 1, [track])
            return _session
        }

        RegistryLookup {
            id: lookup_audio_in
            registry: registries.objects_registry
            key: "tut_direct_in_1"
        }
        property alias audio_in: lookup_audio_in.object

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

            function audio_in() {
                return session.audio_in
            }

            function first_loop_audio_chan() {
                return first_loop().get_audio_output_channels()[0]
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

            // Convenience function to run the backend in controlled mode
            // (sending 0 on audio input), but insert "markers" (specifically-valued samples)
            // at particular times. These can later be checked in output to verify alignment.
            function run_with_marker_samples(total_frames, marker_positions) {
                var sent = 0
                var idx = 0
                while (sent < total_frames && idx < marker_positions.length) {
                    let next_marker = marker_positions[idx]
                    let next_marker_offset = next_marker - sent
                    let remaining = total_frames - sent

                    if (next_marker_offset >= remaining) {
                        // Just send the rest
                        session.backend.dummy_request_controlled_frames(remaining)
                        session.backend.dummy_run_requested_frames()
                        sent += remaining
                    } else {
                        // Send up to the next marker
                        if (next_marker_offset > 0) {
                            session.backend.dummy_request_controlled_frames(next_marker_offset)
                            session.backend.dummy_run_requested_frames()
                            sent += next_marker_offset
                        }

                        // Insert the marker
                        audio_in().dummy_queue_data([0.51])
                        session.backend.dummy_request_controlled_frames(1)
                        session.backend.dummy_run_requested_frames()
                        sent += 1
                        idx += 1
                    }
                }

                if (total_frames - sent > 0) {
                    session.backend.dummy_request_controlled_frames(total_frames - sent)
                    session.backend.dummy_run_requested_frames()
                }
            }

            function verify_markers_at(data, offsets) {
                var result = true
                function compare (a,b) { return (a == b || ((a - b) < Math.max(a,b) / 10000.0)) }
                var found_marker_offsets = []
                for(var idx=0; idx<data.length; idx++) {
                    if (data[idx] > 0.5) {
                        found_marker_offsets.push(idx)
                    }
                }
                verify_eq(found_marker_offsets, offsets)
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
                    clear()

                    // Set up so that sync loop is playing and will cycle 50 samples from now
                    first_loop().create_backend_loop()
                    first_loop().set_length(1000)
                    sync_loop().create_backend_loop()
                    sync_loop().set_length(100)
                    session.backend.dummy_enter_controlled_mode()
                    testcase.wait_controlled_mode(session.backend)
                    sync_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(50)
                    session.backend.dummy_run_requested_frames()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(sync_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(sync_loop().position, 50)

                    registries.state_registry.set_sync_active(true)
                    first_loop().on_play_clicked()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(first_loop().next_transition_delay, 0) // planned transition
                    verify_eq(first_loop().next_mode, ShoopConstants.LoopMode.Playing)

                    // Now proceed to the trigger
                    session.backend.dummy_request_controlled_frames(100)
                    testcase.wait_controlled_mode(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(first_loop().position, 50)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                },

                'test_playback_synced_solo': () => {
                    clear()

                    // Set up so that sync loop is playing and will cycle 50 samples from now
                    first_loop().create_backend_loop()
                    first_loop().set_length(1000)
                    sync_loop().create_backend_loop()
                    sync_loop().set_length(100)
                    second_loop().create_backend_loop()
                    second_loop().set_length(1000)
                    session.backend.dummy_enter_controlled_mode()
                    testcase.wait_controlled_mode(session.backend)

                    sync_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    second_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(50)
                    session.backend.dummy_run_requested_frames()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(sync_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(sync_loop().position, 50)

                    registries.state_registry.set_sync_active(true)
                    registries.state_registry.set_solo_active(true)
                    first_loop().on_play_clicked()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(first_loop().next_transition_delay, 0) // planned transition
                    verify_eq(first_loop().next_mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(second_loop().next_transition_delay, 0)

                    // Now proceed to the trigger
                    session.backend.dummy_request_controlled_frames(100)
                    testcase.wait_controlled_mode(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(first_loop().position, 50)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(second_loop().next_transition_delay, -1)
                },
                
                'test_playback_targeted': () => {
                    clear()

                    // Set up so that sync loop is playing and will cycle 50 samples from now.
                    // Second loop is 3 cycles long and in its first cycle.
                    first_loop().create_backend_loop()
                    first_loop().set_length(1000)
                    second_loop().create_backend_loop()
                    second_loop().set_length(300)
                    sync_loop().create_backend_loop()
                    sync_loop().set_length(100)
                    session.backend.dummy_enter_controlled_mode()
                    testcase.wait_controlled_mode(session.backend)
                    sync_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    second_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(50)
                    session.backend.dummy_run_requested_frames()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(sync_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(sync_loop().position, 50)
                    verify_eq(second_loop().position, 50)

                    registries.state_registry.set_sync_active(true)
                    second_loop().target()
                    first_loop().on_play_clicked()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(first_loop().next_transition_delay, 2) // planned transition
                    verify_eq(first_loop().next_mode, ShoopConstants.LoopMode.Playing)

                    // Now proceed to the trigger
                    session.backend.dummy_request_controlled_frames(300)
                    testcase.wait_controlled_mode(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(first_loop().position, 50)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                },

                'test_playback_targeted_solo': () => {
                    clear()

                    // Set up so that sync loop is playing and will cycle 50 samples from now.
                    // Second loop is 3 cycles long and in its first cycle.
                    first_loop().create_backend_loop()
                    first_loop().set_length(1000)
                    second_loop().create_backend_loop()
                    second_loop().set_length(300)
                    sync_loop().create_backend_loop()
                    sync_loop().set_length(100)
                    session.backend.dummy_enter_controlled_mode()
                    testcase.wait_controlled_mode(session.backend)
                    sync_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    second_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(50)
                    session.backend.dummy_run_requested_frames()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(sync_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(sync_loop().position, 50)
                    verify_eq(second_loop().position, 50)

                    registries.state_registry.set_sync_active(true)
                    registries.state_registry.set_solo_active(true)
                    second_loop().target()
                    first_loop().on_play_clicked()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(first_loop().next_transition_delay, 2) // planned transition
                    verify_eq(first_loop().next_mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(second_loop().next_transition_delay, 2)
                    verify_eq(second_loop().next_mode, ShoopConstants.LoopMode.Stopped)

                    // Now proceed to the trigger
                    session.backend.dummy_request_controlled_frames(300)
                    testcase.wait_controlled_mode(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(first_loop().position, 50)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(second_loop().next_transition_delay, -1) // nothing planned
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
                    clear()

                    // Set up so that sync loop is playing and will cycle 50 samples from now
                    first_loop().create_backend_loop()
                    first_loop().set_length(1000)
                    sync_loop().create_backend_loop()
                    sync_loop().set_length(100)
                    session.backend.dummy_enter_controlled_mode()
                    testcase.wait_controlled_mode(session.backend)
                    sync_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(50)
                    session.backend.dummy_run_requested_frames()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(sync_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(sync_loop().position, 50)

                    registries.state_registry.set_sync_active(true)
                    first_loop().on_playdry_clicked()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(first_loop().next_transition_delay, 0) // planned transition
                    verify_eq(first_loop().next_mode, ShoopConstants.LoopMode.PlayingDryThroughWet)

                    // Now proceed to the trigger
                    session.backend.dummy_request_controlled_frames(100)
                    testcase.wait_controlled_mode(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.PlayingDryThroughWet)
                    verify_eq(first_loop().position, 50)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                },

                'test_playdry_synced_solo': () => {
                    clear()

                    // Set up so that sync loop is playing and will cycle 50 samples from now
                    first_loop().create_backend_loop()
                    first_loop().set_length(1000)
                    sync_loop().create_backend_loop()
                    sync_loop().set_length(100)
                    second_loop().create_backend_loop()
                    second_loop().set_length(1000)
                    session.backend.dummy_enter_controlled_mode()
                    testcase.wait_controlled_mode(session.backend)

                    sync_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    second_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(50)
                    session.backend.dummy_run_requested_frames()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(sync_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(sync_loop().position, 50)

                    registries.state_registry.set_sync_active(true)
                    registries.state_registry.set_solo_active(true)
                    first_loop().on_playdry_clicked()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(first_loop().next_transition_delay, 0) // planned transition
                    verify_eq(first_loop().next_mode, ShoopConstants.LoopMode.PlayingDryThroughWet)
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(second_loop().next_transition_delay, 0)

                    // Now proceed to the trigger
                    session.backend.dummy_request_controlled_frames(100)
                    testcase.wait_controlled_mode(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.PlayingDryThroughWet)
                    verify_eq(first_loop().position, 50)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(second_loop().next_transition_delay, -1)
                },
                
                'test_playdry_targeted': () => {
                    clear()

                    // Set up so that sync loop is playing and will cycle 50 samples from now.
                    // Second loop is 3 cycles long and in its first cycle.
                    first_loop().create_backend_loop()
                    first_loop().set_length(1000)
                    second_loop().create_backend_loop()
                    second_loop().set_length(300)
                    sync_loop().create_backend_loop()
                    sync_loop().set_length(100)
                    session.backend.dummy_enter_controlled_mode()
                    testcase.wait_controlled_mode(session.backend)
                    sync_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    second_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(50)
                    session.backend.dummy_run_requested_frames()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(sync_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(sync_loop().position, 50)
                    verify_eq(second_loop().position, 50)

                    registries.state_registry.set_sync_active(true)
                    second_loop().target()
                    first_loop().on_playdry_clicked()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(first_loop().next_transition_delay, 2) // planned transition
                    verify_eq(first_loop().next_mode, ShoopConstants.LoopMode.PlayingDryThroughWet)

                    // Now proceed to the trigger
                    session.backend.dummy_request_controlled_frames(300)
                    testcase.wait_controlled_mode(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.PlayingDryThroughWet)
                    verify_eq(first_loop().position, 50)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                },

                'test_playdry_targeted_solo': () => {
                    clear()

                    // Set up so that sync loop is playing and will cycle 50 samples from now.
                    // Second loop is 3 cycles long and in its first cycle.
                    first_loop().create_backend_loop()
                    first_loop().set_length(1000)
                    second_loop().create_backend_loop()
                    second_loop().set_length(300)
                    sync_loop().create_backend_loop()
                    sync_loop().set_length(100)
                    session.backend.dummy_enter_controlled_mode()
                    testcase.wait_controlled_mode(session.backend)
                    sync_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    second_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(50)
                    session.backend.dummy_run_requested_frames()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(sync_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(sync_loop().position, 50)
                    verify_eq(second_loop().position, 50)

                    registries.state_registry.set_sync_active(true)
                    registries.state_registry.set_solo_active(true)
                    second_loop().target()
                    first_loop().on_playdry_clicked()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(first_loop().next_transition_delay, 2) // planned transition
                    verify_eq(first_loop().next_mode, ShoopConstants.LoopMode.PlayingDryThroughWet)
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(second_loop().next_transition_delay, 2)
                    verify_eq(second_loop().next_mode, ShoopConstants.LoopMode.Stopped)

                    // Now proceed to the trigger
                    session.backend.dummy_request_controlled_frames(300)
                    testcase.wait_controlled_mode(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.PlayingDryThroughWet)
                    verify_eq(first_loop().position, 50)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(second_loop().next_transition_delay, -1) // nothing planned
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
                    clear()

                    // Set up so that sync loop is playing and will cycle 50 samples from now
                    first_loop().create_backend_loop()
                    first_loop().set_length(1000)
                    sync_loop().create_backend_loop()
                    sync_loop().set_length(100)
                    session.backend.dummy_enter_controlled_mode()
                    testcase.wait_controlled_mode(session.backend)
                    sync_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(50)
                    session.backend.dummy_run_requested_frames()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(sync_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(sync_loop().position, 50)

                    registries.state_registry.set_sync_active(true)
                    registries.state_registry.set_apply_n_cycles(0)
                    first_loop().on_record_clicked()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(first_loop().next_transition_delay, 0) // planned transition
                    verify_eq(first_loop().next_mode, ShoopConstants.LoopMode.Recording)

                    // Now proceed to the trigger
                    session.backend.dummy_request_controlled_frames(100)
                    testcase.wait_controlled_mode(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Recording)
                    verify_eq(first_loop().length, 50)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                },

                'test_record_2_then_play': () => {
                    clear()

                    // Set up so that sync loop is playing and will cycle 50 samples from now
                    first_loop().create_backend_loop()
                    first_loop().set_length(1000)
                    sync_loop().create_backend_loop()
                    sync_loop().set_length(100)
                    session.backend.dummy_enter_controlled_mode()
                    testcase.wait_controlled_mode(session.backend)
                    sync_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(50)
                    session.backend.dummy_run_requested_frames()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(sync_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(sync_loop().position, 50)

                    registries.state_registry.set_sync_active(true)
                    registries.state_registry.set_apply_n_cycles(2)
                    registries.state_registry.set_play_after_record_active(true)
                    first_loop().on_record_clicked()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(first_loop().next_transition_delay, 0) // planned transition
                    verify_eq(first_loop().next_mode, ShoopConstants.LoopMode.Recording)

                    // Now proceed to the trigger
                    session.backend.dummy_request_controlled_frames(100)
                    testcase.wait_controlled_mode(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Recording)
                    verify_eq(first_loop().length, 50)
                    verify_eq(first_loop().next_transition_delay, 1) // planned transition
                    verify_eq(first_loop().next_mode, ShoopConstants.LoopMode.Playing)

                    // Now proceed to the trigger
                    session.backend.dummy_request_controlled_frames(200)
                    testcase.wait_controlled_mode(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(first_loop().position, 50)
                    verify_eq(first_loop().length, 200)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                },

                'test_record_2_then_stop': () => {
                    clear()

                    // Set up so that sync loop is playing and will cycle 50 samples from now
                    first_loop().create_backend_loop()
                    first_loop().set_length(1000)
                    sync_loop().create_backend_loop()
                    sync_loop().set_length(100)
                    session.backend.dummy_enter_controlled_mode()
                    testcase.wait_controlled_mode(session.backend)
                    sync_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(50)
                    session.backend.dummy_run_requested_frames()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(sync_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(sync_loop().position, 50)

                    registries.state_registry.set_sync_active(true)
                    registries.state_registry.set_apply_n_cycles(2)
                    registries.state_registry.set_play_after_record_active(false)
                    first_loop().on_record_clicked()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(first_loop().next_transition_delay, 0) // planned transition
                    verify_eq(first_loop().next_mode, ShoopConstants.LoopMode.Recording)

                    // Now proceed to the trigger
                    session.backend.dummy_request_controlled_frames(100)
                    testcase.wait_controlled_mode(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Recording)
                    verify_eq(first_loop().length, 50)
                    verify_eq(first_loop().next_transition_delay, 1) // planned transition
                    verify_eq(first_loop().next_mode, ShoopConstants.LoopMode.Stopped)

                    // Now proceed to the trigger
                    session.backend.dummy_request_controlled_frames(200)
                    testcase.wait_controlled_mode(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(first_loop().position, 0)
                    verify_eq(first_loop().length, 200)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                },

                'test_record_with_targeted_then_play': () => {
                    clear()

                    // Set up so that sync loop is playing and will cycle 50 samples from now.
                    // Second loop is 3 cycles long and in its first cycle.
                    first_loop().create_backend_loop()
                    first_loop().set_length(1000)
                    second_loop().create_backend_loop()
                    second_loop().set_length(300)
                    sync_loop().create_backend_loop()
                    sync_loop().set_length(100)
                    session.backend.dummy_enter_controlled_mode()
                    testcase.wait_controlled_mode(session.backend)
                    sync_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    second_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(50)
                    session.backend.dummy_run_requested_frames()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(sync_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(sync_loop().position, 50)
                    verify_eq(second_loop().position, 50)

                    registries.state_registry.set_sync_active(true)
                    registries.state_registry.set_play_after_record_active(true)
                    second_loop().target()
                    first_loop().on_record_clicked()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(first_loop().next_transition_delay, 2) // planned transition
                    verify_eq(first_loop().next_mode, ShoopConstants.LoopMode.Recording)

                    // Now proceed to the trigger
                    session.backend.dummy_request_controlled_frames(300)
                    testcase.wait_controlled_mode(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Recording)
                    verify_eq(first_loop().length, 50)
                    verify_eq(first_loop().next_transition_delay, 2) // plan to play
                    verify_eq(first_loop().next_mode, ShoopConstants.LoopMode.Playing)

                    // Now proceed to the trigger
                    session.backend.dummy_request_controlled_frames(300)
                    testcase.wait_controlled_mode(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(first_loop().position, 50)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                },

                'test_record_with_targeted_then_stop': () => {
                    clear()

                    // Set up so that sync loop is playing and will cycle 50 samples from now.
                    // Second loop is 3 cycles long and in its first cycle.
                    first_loop().create_backend_loop()
                    first_loop().set_length(1000)
                    second_loop().create_backend_loop()
                    second_loop().set_length(300)
                    sync_loop().create_backend_loop()
                    sync_loop().set_length(100)
                    session.backend.dummy_enter_controlled_mode()
                    testcase.wait_controlled_mode(session.backend)
                    sync_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    second_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(50)
                    session.backend.dummy_run_requested_frames()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(sync_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(sync_loop().position, 50)
                    verify_eq(second_loop().position, 50)

                    registries.state_registry.set_sync_active(true)
                    registries.state_registry.set_play_after_record_active(false)
                    second_loop().target()
                    first_loop().on_record_clicked()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(first_loop().next_transition_delay, 2) // planned transition
                    verify_eq(first_loop().next_mode, ShoopConstants.LoopMode.Recording)

                    // Now proceed to the trigger
                    session.backend.dummy_request_controlled_frames(300)
                    testcase.wait_controlled_mode(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Recording)
                    verify_eq(first_loop().length, 50)
                    verify_eq(first_loop().next_transition_delay, 2) // plan to play
                    verify_eq(first_loop().next_mode, ShoopConstants.LoopMode.Stopped)

                    // Now proceed to the trigger
                    session.backend.dummy_request_controlled_frames(300)
                    testcase.wait_controlled_mode(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(first_loop().position, 0)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
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

                    session.backend.dummy_enter_controlled_mode()
                    testcase.wait_controlled_mode(session.backend)
                    testcase.wait_updated(session.backend)

                    // One marker at end
                    run_with_marker_samples(500, [499])
                    
                    testcase.wait_updated(session.backend)
                    registries.state_registry.set_play_after_record_active(false)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    first_loop().on_grab_clicked()

                    testcase.wait_updated(session.backend)
                    verify_true(first_loop().length > 0)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                    let data = first_loop_audio_chan().get_data_list().slice(-500)
                    verify_markers_at(data, [data.length - 1])
                },

                'test_grab_ringbuffer_play': () => {
                    clear()

                    session.backend.dummy_enter_controlled_mode()
                    testcase.wait_controlled_mode(session.backend)
                    testcase.wait_updated(session.backend)
                    
                    // One marker at end
                    run_with_marker_samples(500, [499])

                    testcase.wait_updated(session.backend)
                    registries.state_registry.set_play_after_record_active(true)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    first_loop().on_grab_clicked()

                    testcase.wait_updated(session.backend)
                    verify_true(first_loop().length > 0)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                    let data = first_loop_audio_chan().get_data_list().slice(-500)
                    verify_markers_at(data, [data.length - 1])
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
                    first_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    second_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)

                    testcase.wait_updated(session.backend)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Playing)

                    // Now grab and play
                    session.backend.dummy_request_controlled_frames(500);
                    session.backend.dummy_run_requested_frames()
                    testcase.wait_updated(session.backend)
                    registries.state_registry.set_play_after_record_active(true)
                    registries.state_registry.set_solo_active(true)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    first_loop().on_grab_clicked()

                    testcase.wait_updated(session.backend)
                    verify_true(first_loop().length > 0)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(second_loop().next_transition_delay, -1) // nothing planned
                },

                'test_grab_ringbuffer_2_then_play_synced': () => {
                    clear()

                    // Set up so that sync loop is playing and will cycle 50 samples from now
                    first_loop().create_backend_loop()
                    first_loop().set_length(1000)
                    sync_loop().create_backend_loop()
                    sync_loop().set_length(100)
                    session.backend.dummy_enter_controlled_mode()
                    testcase.wait_controlled_mode(session.backend)
                    sync_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    
                    // Run 550 samples with a marker sample at 325 and 475. That corresponds to
                    // the 25th frame of the 2nd-last cycle and 75th frame of the last cycle.
                    // Those should both be in the capture.
                    run_with_marker_samples(550, [325, 475])
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(sync_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(sync_loop().position, 50)

                    registries.state_registry.set_sync_active(true)
                    registries.state_registry.set_apply_n_cycles(2)
                    registries.state_registry.set_play_after_record_active(true)
                    first_loop().on_grab_clicked()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                    verify_eq(first_loop().position, 50)
                    verify_eq(first_loop().length, 200)
                    let start_offset = first_loop_audio_chan().start_offset
                    let data = first_loop_audio_chan().get_data_list().slice(start_offset)
                    verify_markers_at(data, [25, 175])
                },

                'test_grab_ringbuffer_2_then_play_unsynced': () => {
                    clear()

                    // Set up so that sync loop is playing and will cycle 50 samples from now
                    first_loop().create_backend_loop()
                    first_loop().set_length(1000)
                    sync_loop().create_backend_loop()
                    sync_loop().set_length(100)
                    session.backend.dummy_enter_controlled_mode()
                    testcase.wait_controlled_mode(session.backend)
                    sync_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)

                    // Run 550 samples with a marker sample at 425 and 510. That corresponds to
                    // the 25th frame of the last cycle and 10th frame of the current cycle.
                    run_with_marker_samples(550, [425, 510])
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(sync_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(sync_loop().position, 50)

                    registries.state_registry.set_sync_active(false)
                    registries.state_registry.set_apply_n_cycles(2)
                    registries.state_registry.set_play_after_record_active(true)
                    first_loop().on_grab_clicked()
                    testcase.wait_updated(session.backend)

                    // The interpretation of "no sync" for grabbing is that the currently
                    // running cycle is the last one being recorded. Recording should
                    // immediately continue until the end of the cycle.

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Recording)
                    verify_eq(first_loop().next_transition_delay, 0) // record the remainder
                    verify_eq(first_loop().next_mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(first_loop().length, 150)

                    // Perform the transition
                    session.backend.dummy_request_controlled_frames(100)
                    session.backend.dummy_run_requested_frames()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                    verify_eq(first_loop().length, 200)
                    verify_eq(first_loop().position, 50)
                    let start_offset = first_loop_audio_chan().start_offset
                    let data = first_loop_audio_chan().get_data_list().slice(start_offset)
                    verify_markers_at(data, [25, 110])
                },

                'test_grab_ringbuffer_with_targeted_then_play_unsynced': () => {
                    clear()

                    // Set up so that sync loop is playing and will cycle 50 samples from now
                    first_loop().create_backend_loop()
                    first_loop().set_length(1000)
                    second_loop().create_backend_loop()
                    second_loop().set_length(300)
                    sync_loop().create_backend_loop()
                    sync_loop().set_length(100)
                    session.backend.dummy_enter_controlled_mode()
                    testcase.wait_controlled_mode(session.backend)
                    sync_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    second_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    
                    // Run 550 samples with a marker sample at 301 and 410. That corresponds to
                    // the 1st frame of the last cycle and 10th frame of the current cycle.
                    run_with_marker_samples(450, [301, 410])
                    testcase.wait_updated(session.backend)

                    // Second loop is in its 2nd cycle of 3
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(sync_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(sync_loop().position, 50)
                    verify_eq(second_loop().position, 150)

                    registries.state_registry.set_sync_active(false)
                    registries.state_registry.set_apply_n_cycles(2)
                    registries.state_registry.set_play_after_record_active(true)
                    second_loop().target()
                    first_loop().on_grab_clicked()
                    testcase.wait_updated(session.backend)

                    // The interpretation of "no sync" for grabbing is that the currently
                    // running cycle is the last one being recorded. Recording should
                    // immediately continue until the end of the cycle.
                    // In the targeting use-case, that means the grabbed length should be
                    // equal to the targeted loop's and the loop should transition to recording
                    // until the targeted loop restarts.

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Recording)
                    verify_eq(first_loop().next_transition_delay, 1) // record the remainder
                    verify_eq(first_loop().next_mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(first_loop().length, 150)
                    let start_offset = first_loop_audio_chan().start_offset
                    let data = first_loop_audio_chan().get_data_list().slice(start_offset)
                    verify_markers_at(data, [1, 110])

                    // Perform the transition
                    session.backend.dummy_request_controlled_frames(200)
                    session.backend.dummy_run_requested_frames()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                    verify_eq(first_loop().length, 300)
                    verify_eq(first_loop().position, 50)
                },

                'test_grab_ringbuffer_with_targeted_then_play_synced': () => {
                    clear()

                    // Set up so that sync loop is playing and will cycle 50 samples from now
                    first_loop().create_backend_loop()
                    first_loop().set_length(1000)
                    second_loop().create_backend_loop()
                    second_loop().set_length(300)
                    sync_loop().create_backend_loop()
                    sync_loop().set_length(100)
                    session.backend.dummy_enter_controlled_mode()
                    testcase.wait_controlled_mode(session.backend)
                    sync_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    second_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)

                    // Run 450 samples with a marker sample at 1, 150 and 299. That corresponds to
                    // the 1st frame of the first cycle, 50th frame of the 2nd cycle and 99th frame
                    // of the 3rd cycle of the targeted loop's _previous_ cycle.
                    run_with_marker_samples(450, [1, 150, 299])
                    testcase.wait_updated(session.backend)

                    // Second loop is in its 2nd cycle of 3
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(sync_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(sync_loop().position, 50)
                    verify_eq(second_loop().position, 150)

                    registries.state_registry.set_sync_active(true)
                    registries.state_registry.set_apply_n_cycles(2)
                    registries.state_registry.set_play_after_record_active(true)
                    second_loop().target()
                    first_loop().on_grab_clicked()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                    verify_eq(first_loop().length, 300)
                    verify_eq(first_loop().position, 150)
                    let start_offset = first_loop_audio_chan().start_offset
                    let data = first_loop_audio_chan().get_data_list().slice(start_offset)
                    verify_markers_at(data, [1, 150, 299])
                },

                'test_stop_immediate': () => {
                    clear()

                    first_loop().create_backend_loop()
                    first_loop().set_length(48000)
                    registries.state_registry.set_sync_active(false)
                    first_loop().transition(ShoopConstants.LoopMode.Recording, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Recording)

                    first_loop().on_stop_clicked()
                    testcase.wait_updated(session.backend)
                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                },

                'test_stop_synced': () => {
                    clear()

                    // Set up so that sync and first loop is playing and sync will cycle 50 samples from now
                    first_loop().create_backend_loop()
                    first_loop().set_length(1000)
                    sync_loop().create_backend_loop()
                    sync_loop().set_length(100)
                    session.backend.dummy_enter_controlled_mode()
                    testcase.wait_controlled_mode(session.backend)
                    sync_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    first_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(50)
                    session.backend.dummy_run_requested_frames()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(sync_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(sync_loop().position, 50)

                    registries.state_registry.set_sync_active(true)
                    first_loop().on_stop_clicked()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(first_loop().next_transition_delay, 0) // planned transition
                    verify_eq(first_loop().next_mode, ShoopConstants.LoopMode.Stopped)

                    // Now proceed to the trigger
                    session.backend.dummy_request_controlled_frames(100)
                    testcase.wait_controlled_mode(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(first_loop().position, 0)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                },

                'test_stop_with_targeted': () => {
                    clear()

                    // Set up so that sync loop is playing and will cycle 50 samples from now.
                    // First loop is playing.
                    // Second loop is 3 cycles long and in its first cycle.
                    first_loop().create_backend_loop()
                    first_loop().set_length(1000)
                    second_loop().create_backend_loop()
                    second_loop().set_length(300)
                    sync_loop().create_backend_loop()
                    sync_loop().set_length(100)
                    session.backend.dummy_enter_controlled_mode()
                    testcase.wait_controlled_mode(session.backend)
                    sync_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    first_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    second_loop().transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(50)
                    session.backend.dummy_run_requested_frames()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(sync_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(second_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(sync_loop().position, 50)
                    verify_eq(first_loop().position, 50)
                    verify_eq(second_loop().position, 50)

                    registries.state_registry.set_sync_active(true)
                    second_loop().target()
                    first_loop().on_stop_clicked()
                    testcase.wait_updated(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Playing)
                    verify_eq(first_loop().next_transition_delay, 2) // planned transition
                    verify_eq(first_loop().next_mode, ShoopConstants.LoopMode.Stopped)

                    // Now proceed to the trigger
                    session.backend.dummy_request_controlled_frames(300)
                    testcase.wait_controlled_mode(session.backend)

                    verify_eq(first_loop().mode, ShoopConstants.LoopMode.Stopped)
                    verify_eq(first_loop().position, 0)
                    verify_eq(first_loop().next_transition_delay, -1) // nothing planned
                },
            })
        }
    }
}