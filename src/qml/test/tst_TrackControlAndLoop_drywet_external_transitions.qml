import QtQuick 6.6

import ShoopDaLoop.Rust
import '../js/generate_session.js' as GenerateSession
import './testfilename.js' as TestFilename
import '..'

ShoopTestFile {
    TestSession {
        id: session
        anchors.fill: parent
        initial_descriptor: {
            let track = GenerateSession.generate_default_track(
                "transition",
                1,
                "transition",
                false,
                "transition",
                1,
                1,
                0,
                true,
                false,
                true,
                undefined
            )
            return GenerateSession.generate_default_session(
                global_args.version_string,
                null,
                true,
                1,
                1,
                [track]
            )
        }

        ShoopSessionTestCase {
            id: testcase
            name: 'TrackControlAndLoop_drywet_external_transitions'
            filename: TestFilename.test_filename()
            session: session

            property var track: session.main_tracks[0]
            property var loop: track.loops[0]
            property var sync_loop: session.sync_track.loops[0]

            RegistryLookup {
                id: audio_input_lookup
                registry: AppRegistries.objects_registry
                key: "transition_audio_dry_in_1"
            }
            property alias audio_input: audio_input_lookup.object

            RegistryLookup {
                id: audio_send_lookup
                registry: AppRegistries.objects_registry
                key: "transition_audio_dry_send_1"
            }
            property alias audio_send: audio_send_lookup.object

            RegistryLookup {
                id: midi_input_lookup
                registry: AppRegistries.objects_registry
                key: "transition_dry_midi_in"
            }
            property alias midi_input: midi_input_lookup.object

            RegistryLookup {
                id: midi_send_lookup
                registry: AppRegistries.objects_registry
                key: "transition_dry_midi_send"
            }
            property alias midi_send: midi_send_lookup.object

            RegistryLookup {
                id: wet_return_lookup
                registry: AppRegistries.objects_registry
                key: "transition_audio_wet_return_1"
            }
            property alias wet_return: wet_return_lookup.object

            RegistryLookup {
                id: wet_output_lookup
                registry: AppRegistries.objects_registry
                key: "transition_audio_wet_out_1"
            }
            property alias wet_output: wet_output_lookup.object

            testcase_init_fn: () => {
                session.backend.dummy_enter_controlled_mode()
                testcase.wait_controlled_mode(session.backend)
                verify_true(track)
                verify_true(loop)
                verify_true(sync_loop)
                verify_true(audio_input)
                verify_true(audio_send)
                verify_true(midi_input)
                verify_true(midi_send)
                verify_true(wet_return)
                verify_true(wet_output)
                loop.create_backend_loop()
                sync_loop.create_backend_loop()
                reset()
            }

            function dry_channel() {
                return loop.get_audio_channels().find((channel) => channel.obj_id.match(/.*_dry_.*/))
            }

            function wet_channel() {
                return loop.get_audio_channels().find((channel) => channel.obj_id.match(/.*_wet_.*/))
            }

            function midi_channel() {
                return loop.midi_channels[0]
            }

            function reset_loop(target) {
                target.transition(
                    ShoopRustConstants.LoopMode.Stopped,
                    ShoopRustConstants.DontWaitForSync,
                    ShoopRustConstants.DontAlignToSyncImmediately
                )
                testcase.wait_updated(session.backend)
                target.clear(0)
                target.midi_channels.forEach((channel) => channel.reset_state_tracking())
            }

            function reset() {
                track.control_widget.monitor = false
                track.control_widget.mute = false
                session.sync_track.control_widget.monitor = false
                reset_loop(loop)
                reset_loop(sync_loop)
                midi_input.dummy_clear_queues()
                midi_send.dummy_clear_queues()
                session.backend.wait_process()
                testcase.wait_updated(session.backend)
            }

            function process(n_frames) {
                session.backend.dummy_request_controlled_frames(n_frames)
                session.backend.dummy_run_requested_frames()
                testcase.wait_updated(session.backend)
            }

            function start_sync_at_position_two() {
                sync_loop.queue_set_length(4)
                sync_loop.transition(
                    ShoopRustConstants.LoopMode.Playing,
                    ShoopRustConstants.DontWaitForSync,
                    ShoopRustConstants.DontAlignToSyncImmediately
                )
                testcase.wait_updated(session.backend)
                process(2)
                verify_eq(sync_loop.position, 2)
            }

            function request_all(n_frames) {
                audio_send.dummy_request_data(n_frames)
                midi_send.dummy_request_data(n_frames)
                wet_output.dummy_request_data(n_frames)
            }

            test_fns: ({
                // Purpose: Synchronized recording must mark pre-roll and capture the first boundary frame.
                // Use case: External dry/wet recording is launched midway through a running sync cycle.
                'test_synchronized_stopped_to_recording_boundary': () => {
                    check_backend()
                    reset()
                    start_sync_at_position_two()
                    loop.transition(
                        ShoopRustConstants.LoopMode.Recording,
                        0,
                        ShoopRustConstants.DontAlignToSyncImmediately
                    )
                    testcase.wait_updated(session.backend)

                    audio_input.dummy_queue_audio_data([1, 2])
                    midi_input.dummy_queue_midi_msgs([
                        { 'time': 0, 'data': [0x90, 70, 100] },
                        { 'time': 1, 'data': [0x80, 70, 0] }
                    ])
                    wet_return.dummy_queue_audio_data([10, 20])
                    request_all(2)
                    process(2)
                    let pre_audio_send = audio_send.dummy_dequeue_audio_data(2)
                    let pre_midi_send = midi_send.dummy_dequeue_midi_msgs()
                    let pre_output = wet_output.dummy_dequeue_audio_data(2)
                    verify_eq(loop.mode, ShoopRustConstants.LoopMode.Recording)

                    audio_input.dummy_queue_audio_data([3, 4, 5, 6])
                    midi_input.dummy_queue_midi_msgs([
                        { 'time': 0, 'data': [0x90, 71, 100] },
                        { 'time': 3, 'data': [0x80, 71, 0] }
                    ])
                    wet_return.dummy_queue_audio_data([30, 40, 50, 60])
                    request_all(4)
                    process(4)

                    verify_eq(pre_audio_send, [0, 0])
                    verify_eq(pre_midi_send, [], null, true)
                    verify_eq(pre_output, [0, 0])
                    verify_eq(audio_send.dummy_dequeue_audio_data(4), [3, 4, 5, 6])
                    verify_eq(midi_send.dummy_dequeue_midi_msgs(), [
                        { 'time': 0, 'data': [0x90, 71, 100] },
                        { 'time': 3, 'data': [0x80, 71, 0] }
                    ], null, true)
                    verify_eq(wet_output.dummy_dequeue_audio_data(4), [0, 0, 0, 0])
                    verify_eq(dry_channel().start_offset, 2)
                    verify_eq(wet_channel().start_offset, 2)
                    verify_eq(dry_channel().get_data(), [1, 2, 3, 4, 5, 6])
                    verify_eq(wet_channel().get_data(), [10, 20, 30, 40, 50, 60])
                    verify_eq(midi_channel().get_recorded_midi_msgs(), [
                        { 'time': 0, 'data': [0x90, 70, 100] },
                        { 'time': 1, 'data': [0x80, 70, 0] },
                        { 'time': 2, 'data': [0x90, 71, 100] },
                        { 'time': 5, 'data': [0x80, 71, 0] }
                    ], null, true)
                },

                // Purpose: Recording-to-playing must close live sends exactly at the sync boundary.
                // Use case: A performer finishes recording while holding a note and continues playing live.
                'test_synchronized_recording_to_playing_boundary': () => {
                    check_backend()
                    reset()
                    sync_loop.queue_set_length(4)
                    sync_loop.transition(
                        ShoopRustConstants.LoopMode.Playing,
                        ShoopRustConstants.DontWaitForSync,
                        ShoopRustConstants.DontAlignToSyncImmediately
                    )
                    loop.transition(
                        ShoopRustConstants.LoopMode.Recording,
                        ShoopRustConstants.DontWaitForSync,
                        ShoopRustConstants.DontAlignToSyncImmediately
                    )
                    testcase.wait_updated(session.backend)

                    audio_input.dummy_queue_audio_data([1, 2])
                    midi_input.dummy_queue_midi_msgs([
                        { 'time': 0, 'data': [0x90, 72, 100] }
                    ])
                    wet_return.dummy_queue_audio_data([10, 20])
                    audio_send.dummy_request_data(2)
                    midi_send.dummy_request_data(2)
                    process(2)
                    audio_send.dummy_dequeue_audio_data(2)
                    midi_send.dummy_dequeue_midi_msgs()

                    loop.transition(
                        ShoopRustConstants.LoopMode.Playing,
                        0,
                        ShoopRustConstants.DontAlignToSyncImmediately
                    )
                    testcase.wait_updated(session.backend)
                    audio_input.dummy_queue_audio_data([3, 4])
                    midi_input.dummy_queue_midi_msgs([])
                    wet_return.dummy_queue_audio_data([30, 40])
                    request_all(2)
                    process(2)
                    let pre_audio_send = audio_send.dummy_dequeue_audio_data(2)
                    let boundary_midi = midi_send.dummy_dequeue_midi_msgs()
                    let pre_output = wet_output.dummy_dequeue_audio_data(2)
                    verify_eq(loop.mode, ShoopRustConstants.LoopMode.Playing)

                    audio_input.dummy_queue_audio_data([5, 6, 7, 8])
                    midi_input.dummy_queue_midi_msgs([
                        { 'time': 0, 'data': [0x90, 73, 100] },
                        { 'time': 3, 'data': [0x80, 73, 0] }
                    ])
                    wet_return.dummy_queue_audio_data([50, 60, 70, 80])
                    request_all(4)
                    process(4)

                    verify_eq(pre_audio_send, [3, 4])
                    verify_eq(boundary_midi, [], null, true)
                    verify_eq(midi_send.midi_n_output_notes_active, 0)
                    verify_eq(pre_output, [0, 0])
                    verify_eq(audio_send.dummy_dequeue_audio_data(4), [0, 0, 0, 0])
                    verify_true(!midi_send.dummy_dequeue_midi_msgs().some((message) => message.data[1] === 73))
                    verify_eq(wet_output.dummy_dequeue_audio_data(4), [10, 20, 30, 40])
                    verify_eq(dry_channel().get_data(), [1, 2, 3, 4])
                    verify_eq(wet_channel().get_data(), [10, 20, 30, 40])
                },

                // Purpose: Dry re-recording must start dry sends and wet replacement on the same sync frame.
                // Use case: A stored loop is committed through external effects without leaking live input.
                'test_synchronized_playing_to_rerecord_boundary': () => {
                    check_backend()
                    reset()
                    dry_channel().load_data([1, 2, 3, 4])
                    wet_channel().load_data([50, 60, 70, 80])
                    midi_channel().load_midi_data([
                        { 'time': 1, 'data': [0x90, 81, 100] },
                        { 'time': 2, 'data': [0x80, 81, 0] }
                    ])
                    loop.queue_set_length(4)
                    sync_loop.queue_set_length(4)
                    sync_loop.transition(
                        ShoopRustConstants.LoopMode.Playing,
                        ShoopRustConstants.DontWaitForSync,
                        ShoopRustConstants.DontAlignToSyncImmediately
                    )
                    loop.transition(
                        ShoopRustConstants.LoopMode.Playing,
                        ShoopRustConstants.DontWaitForSync,
                        ShoopRustConstants.DontAlignToSyncImmediately
                    )
                    testcase.wait_updated(session.backend)
                    process(2)

                    loop.transition(
                        ShoopRustConstants.LoopMode.RecordingDryIntoWet,
                        0,
                        ShoopRustConstants.DontAlignToSyncImmediately
                    )
                    testcase.wait_updated(session.backend)
                    audio_input.dummy_queue_audio_data([9, 10])
                    midi_input.dummy_queue_midi_msgs([
                        { 'time': 0, 'data': [0x90, 90, 100] }
                    ])
                    wet_return.dummy_queue_audio_data([10, 20])
                    request_all(2)
                    process(2)
                    let pre_audio_send = audio_send.dummy_dequeue_audio_data(2)
                    let boundary_midi = midi_send.dummy_dequeue_midi_msgs()
                    let pre_output = wet_output.dummy_dequeue_audio_data(2)
                    verify_eq(loop.mode, ShoopRustConstants.LoopMode.RecordingDryIntoWet)

                    audio_input.dummy_queue_audio_data([11, 12, 13, 14])
                    midi_input.dummy_queue_midi_msgs([
                        { 'time': 3, 'data': [0x80, 90, 0] }
                    ])
                    wet_return.dummy_queue_audio_data([30, 40, 50, 60])
                    request_all(4)
                    process(4)

                    verify_eq(pre_audio_send, [0, 0])
                    verify_eq(boundary_midi, [], null, true)
                    verify_eq(pre_output, [70, 80])
                    verify_eq(audio_send.dummy_dequeue_audio_data(4), [1, 2, 3, 4])
                    verify_eq(midi_send.dummy_dequeue_midi_msgs(), [
                        { 'time': 1, 'data': [0x90, 81, 100] },
                        { 'time': 2, 'data': [0x80, 81, 0] }
                    ], null, true)
                    verify_eq(wet_output.dummy_dequeue_audio_data(4), [30, 40, 50, 60])

                    loop.transition(
                        ShoopRustConstants.LoopMode.Stopped,
                        ShoopRustConstants.DontWaitForSync,
                        ShoopRustConstants.DontAlignToSyncImmediately
                    )
                    testcase.wait_updated(session.backend)
                    process(1)
                    verify_eq(wet_channel().get_data(), [30, 40, 50, 60])
                }
            })
        }
    }
}
