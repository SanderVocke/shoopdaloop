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
                "multiple",
                3,
                "multiple",
                false,
                "multiple",
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
            name: 'TrackControlAndLoop_drywet_external_multiple'
            filename: TestFilename.test_filename()
            session: session

            property var track: session.main_tracks[0]
            property var loops: track.loops

            RegistryLookup {
                id: audio_input_lookup
                registry: AppRegistries.objects_registry
                key: "multiple_audio_dry_in_1"
            }
            property alias audio_input: audio_input_lookup.object

            RegistryLookup {
                id: audio_send_lookup
                registry: AppRegistries.objects_registry
                key: "multiple_audio_dry_send_1"
            }
            property alias audio_send: audio_send_lookup.object

            RegistryLookup {
                id: midi_input_lookup
                registry: AppRegistries.objects_registry
                key: "multiple_dry_midi_in"
            }
            property alias midi_input: midi_input_lookup.object

            RegistryLookup {
                id: midi_send_lookup
                registry: AppRegistries.objects_registry
                key: "multiple_dry_midi_send"
            }
            property alias midi_send: midi_send_lookup.object

            RegistryLookup {
                id: wet_return_lookup
                registry: AppRegistries.objects_registry
                key: "multiple_audio_wet_return_1"
            }
            property alias wet_return: wet_return_lookup.object

            RegistryLookup {
                id: wet_output_lookup
                registry: AppRegistries.objects_registry
                key: "multiple_audio_wet_out_1"
            }
            property alias wet_output: wet_output_lookup.object

            testcase_init_fn: () => {
                session.backend.dummy_enter_controlled_mode()
                testcase.wait_controlled_mode(session.backend)
                verify_true(track)
                verify_eq(loops.length, 3)
                verify_true(audio_input)
                verify_true(audio_send)
                verify_true(midi_input)
                verify_true(midi_send)
                verify_true(wet_return)
                verify_true(wet_output)
                loops.forEach((loop) => loop.create_backend_loop())
                reset()
            }

            function audio_channel(loop, kind) {
                return loop.get_audio_channels()
                    .find((channel) => channel.obj_id.match(new RegExp(`.*_${kind}_.*`)))
            }

            function midi_channel(loop) {
                return loop.midi_channels[0]
            }

            function reset_loop(loop) {
                loop.transition(
                    ShoopRustConstants.LoopMode.Stopped,
                    ShoopRustConstants.DontWaitForSync,
                    ShoopRustConstants.DontAlignToSyncImmediately
                )
                testcase.wait_updated(session.backend)
                loop.clear(0)
                loop.midi_channels.forEach((channel) => channel.reset_state_tracking())
            }

            function reset() {
                track.control_widget.monitor = false
                track.control_widget.mute = false
                loops.forEach((loop) => reset_loop(loop))
                midi_input.dummy_clear_queues()
                midi_send.dummy_clear_queues()
                session.backend.wait_process()
                testcase.wait_updated(session.backend)
            }

            function load_loop(loop, dry, wet, midi) {
                audio_channel(loop, "dry").load_data(dry)
                audio_channel(loop, "wet").load_data(wet)
                midi_channel(loop).load_midi_data(midi)
                loop.queue_set_length(dry.length)
            }

            function transition(loop, mode) {
                loop.transition(
                    mode,
                    ShoopRustConstants.DontWaitForSync,
                    ShoopRustConstants.DontAlignToSyncImmediately
                )
            }

            function run_cycle(input, midi, returned) {
                audio_input.dummy_queue_audio_data(input)
                midi_input.dummy_queue_midi_msgs(midi)
                wet_return.dummy_queue_audio_data(returned)
                audio_send.dummy_request_data(input.length)
                midi_send.dummy_request_data(input.length)
                wet_output.dummy_request_data(input.length)
                session.backend.dummy_request_controlled_frames(input.length)
                session.backend.dummy_run_requested_frames()
                testcase.wait_updated(session.backend)
                return {
                    "audio_send": audio_send.dummy_dequeue_audio_data(input.length),
                    "midi_send": midi_send.dummy_dequeue_midi_msgs(),
                    "wet_output": wet_output.dummy_dequeue_audio_data(input.length)
                }
            }

            function stop_and_publish(loop) {
                transition(loop, ShoopRustConstants.LoopMode.Stopped)
                testcase.wait_updated(session.backend)
                session.backend.dummy_request_controlled_frames(1)
                session.backend.dummy_run_requested_frames()
                testcase.wait_updated(session.backend)
            }

            test_fns: ({
                // Purpose: Recording on one loop must open live sends while another loop plays stored wet audio.
                // Use case: A performer layers a new externally processed loop over an existing wet loop.
                'test_recording_loop_while_another_plays_wet': () => {
                    check_backend()
                    reset()
                    load_loop(loops[1], [5, 6, 7, 8], [50, 60, 70, 80], [])
                    transition(loops[0], ShoopRustConstants.LoopMode.Recording)
                    transition(loops[1], ShoopRustConstants.LoopMode.Playing)
                    testcase.wait_updated(session.backend)

                    let live_midi = [
                        { 'time': 0, 'data': [0x90, 70, 100] },
                        { 'time': 3, 'data': [0x80, 70, 0] }
                    ]
                    let result = run_cycle([1, 2, 3, 4], live_midi, [10, 20, 30, 40])
                    verify_eq(result.audio_send, [1, 2, 3, 4])
                    verify_eq(result.midi_send, live_midi, null, true)
                    verify_eq(result.wet_output, [50, 60, 70, 80])

                    stop_and_publish(loops[0])
                    verify_eq(audio_channel(loops[0], "dry").get_data(), [1, 2, 3, 4])
                    verify_eq(audio_channel(loops[0], "wet").get_data(), [10, 20, 30, 40])
                    verify_eq(audio_channel(loops[1], "wet").get_data(), [50, 60, 70, 80])
                },

                // Purpose: Wet playback and dry playback must share output while only stored dry data is sent.
                // Use case: One loop plays its committed wet take while another is auditioned through external FX.
                'test_wet_playback_alongside_dry_playback': () => {
                    check_backend()
                    reset()
                    let dry_midi = [
                        { 'time': 1, 'data': [0x90, 81, 100] },
                        { 'time': 2, 'data': [0x80, 81, 0] }
                    ]
                    load_loop(loops[0], [1, 1, 1, 1], [50, 60, 70, 80], [])
                    load_loop(loops[1], [5, 6, 7, 8], [90, 90, 90, 90], dry_midi)
                    transition(loops[0], ShoopRustConstants.LoopMode.Playing)
                    transition(loops[1], ShoopRustConstants.LoopMode.PlayingDryThroughWet)
                    testcase.wait_updated(session.backend)

                    let live_midi = [
                        { 'time': 0, 'data': [0x90, 90, 100] },
                        { 'time': 3, 'data': [0x80, 90, 0] }
                    ]
                    let result = run_cycle([1, 2, 3, 4], live_midi, [10, 20, 30, 40])
                    verify_eq(result.audio_send, [5, 6, 7, 8])
                    verify_eq(result.midi_send, dry_midi, null, true)
                    verify_eq(result.wet_output, [60, 80, 100, 120])
                    verify_eq(audio_channel(loops[0], "wet").get_data(), [50, 60, 70, 80])
                    verify_eq(audio_channel(loops[1], "wet").get_data(), [90, 90, 90, 90])
                },

                // Purpose: Wet playback must coexist with another loop replacing wet content from stored dry data.
                // Use case: A performer commits new external FX for one loop while another wet loop keeps playing.
                'test_wet_playback_alongside_dry_rerecord': () => {
                    check_backend()
                    reset()
                    let dry_midi = [
                        { 'time': 1, 'data': [0x90, 82, 100] },
                        { 'time': 2, 'data': [0x80, 82, 0] }
                    ]
                    load_loop(loops[0], [1, 1, 1, 1], [50, 60, 70, 80], [])
                    load_loop(loops[1], [5, 6, 7, 8], [90, 90, 90, 90], dry_midi)
                    transition(loops[0], ShoopRustConstants.LoopMode.Playing)
                    transition(loops[1], ShoopRustConstants.LoopMode.RecordingDryIntoWet)
                    testcase.wait_updated(session.backend)

                    let live_midi = [
                        { 'time': 0, 'data': [0x90, 90, 100] },
                        { 'time': 3, 'data': [0x80, 90, 0] }
                    ]
                    let result = run_cycle([1, 2, 3, 4], live_midi, [10, 20, 30, 40])
                    verify_eq(result.audio_send, [5, 6, 7, 8])
                    verify_eq(result.midi_send, dry_midi, null, true)
                    verify_eq(result.wet_output, [60, 80, 100, 120])

                    stop_and_publish(loops[1])
                    verify_eq(audio_channel(loops[1], "dry").get_data(), [5, 6, 7, 8])
                    verify_eq(audio_channel(loops[1], "wet").get_data(), [10, 20, 30, 40])
                    verify_eq(audio_channel(loops[0], "wet").get_data(), [50, 60, 70, 80])
                }
            })
        }
    }
}
