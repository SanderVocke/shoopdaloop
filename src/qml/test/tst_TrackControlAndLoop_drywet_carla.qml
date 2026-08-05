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
            let rack = GenerateSession.generate_default_track(
                "rack", 1, "rack", false, "rack", 2, 2, 0,
                true, false, false, "carla_rack"
            )
            let patchbay = GenerateSession.generate_default_track(
                "patchbay", 1, "patchbay", false, "patchbay", 2, 2, 0,
                true, false, false, "carla_patchbay"
            )
            let patchbay16 = GenerateSession.generate_default_track(
                "patchbay16", 1, "patchbay16", false, "patchbay16", 16, 16, 0,
                true, false, false, "carla_patchbay_16"
            )
            return GenerateSession.generate_default_session(
                global_args.version_string,
                null,
                true,
                1,
                1,
                [rack, patchbay, patchbay16]
            )
        }

        ShoopSessionTestCase {
            id: testcase
            name: 'TrackControlAndLoop_drywet_carla'
            filename: TestFilename.test_filename()
            session: session

            property var tracks: session.main_tracks

            testcase_init_fn: () => {
                session.backend.dummy_enter_controlled_mode()
                testcase.wait_controlled_mode(session.backend)
                verify_eq(tracks.length, 3)
                tracks.forEach((track) => {
                    verify_true(track)
                    verify_true(track.loops[0])
                    track.loops[0].create_backend_loop()
                    verify_true(fx(track))
                    verify_true(fx(track).initialized)
                })
                reset_all()
            }

            function fx(track) {
                return AppRegistries.objects_registry.get(`${track.obj_id}_fx_chain`)
            }

            function port(track, suffix) {
                return AppRegistries.objects_registry.get(`${track.obj_id}_${suffix}`)
            }

            function dry_channel(loop) {
                return loop.get_audio_channels().find((channel) => channel.obj_id.match(/.*_dry_.*/))
            }

            function wet_channel(loop) {
                return loop.get_audio_channels().find((channel) => channel.obj_id.match(/.*_wet_.*/))
            }

            function process(n_frames) {
                session.backend.dummy_request_controlled_frames(n_frames)
                session.backend.dummy_run_requested_frames()
                testcase.wait_updated(session.backend)
            }

            function reset_track(track) {
                let loop = track.loops[0]
                track.control_widget.monitor = false
                track.control_widget.mute = false
                loop.transition(
                    ShoopRustConstants.LoopMode.Stopped,
                    ShoopRustConstants.DontWaitForSync,
                    ShoopRustConstants.DontAlignToSyncImmediately
                )
                testcase.wait_updated(session.backend)
                loop.clear(0)
                loop.midi_channels.forEach((channel) => channel.reset_state_tracking())
                port(track, "dry_midi_in").dummy_clear_queues()
            }

            function reset_all() {
                tracks.forEach((track) => reset_track(track))
                session.backend.wait_process()
                testcase.wait_updated(session.backend)
            }

            function transition(loop, mode) {
                loop.transition(
                    mode,
                    ShoopRustConstants.DontWaitForSync,
                    ShoopRustConstants.DontAlignToSyncImmediately
                )
                testcase.wait_updated(session.backend)
            }

            function prepare_content(loop) {
                dry_channel(loop).load_data([1, 2, 3, 4])
                wet_channel(loop).load_data([5, 6, 7, 8])
                loop.midi_channels[0].load_midi_data([
                    { 'time': 1, 'data': [0x90, 70, 100] },
                    { 'time': 2, 'data': [0x80, 70, 0] }
                ])
                loop.queue_set_length(4)
                testcase.wait_updated(session.backend)
            }

            function verify_activation_modes(track) {
                reset_track(track)
                let loop = track.loops[0]
                let chain = fx(track)
                prepare_content(loop)

                verify_eq(chain.active, false)
                track.control_widget.monitor = true
                testcase.wait_updated(session.backend)
                verify_eq(chain.active, true)
                track.control_widget.monitor = false
                testcase.wait_updated(session.backend)
                verify_eq(chain.active, false)

                transition(loop, ShoopRustConstants.LoopMode.Recording)
                verify_eq(chain.active, true)
                transition(loop, ShoopRustConstants.LoopMode.Stopped)
                prepare_content(loop)
                transition(loop, ShoopRustConstants.LoopMode.Replacing)
                verify_eq(chain.active, true)
                transition(loop, ShoopRustConstants.LoopMode.Stopped)
                prepare_content(loop)
                transition(loop, ShoopRustConstants.LoopMode.Playing)
                verify_eq(chain.active, false)
                track.control_widget.monitor = true
                testcase.wait_updated(session.backend)
                verify_eq(chain.active, true)
                track.control_widget.monitor = false
                transition(loop, ShoopRustConstants.LoopMode.PlayingDryThroughWet)
                verify_eq(chain.active, true)
                transition(loop, ShoopRustConstants.LoopMode.RecordingDryIntoWet)
                verify_eq(chain.active, true)
                verify_eq(track.control_widget.monitor, false)

                transition(loop, ShoopRustConstants.LoopMode.Stopped)
                let audio_input = port(track, "audio_dry_in_1")
                let wet_output = port(track, "audio_wet_out_1")
                audio_input.dummy_queue_audio_data([1, 0, 0, 0])
                wet_output.dummy_request_data(4)
                process(4)
                verify_eq(wet_output.dummy_dequeue_audio_data(4), [0, 0, 0, 0])
            }

            function verify_midi_gating(track) {
                reset_track(track)
                let external_input = port(track, "dry_midi_in")
                let fx_input = port(track, "fx_chain_midi_in_1")
                let stopped_event = [
                    { 'time': 0, 'data': [0x90, 71, 100] }
                ]
                external_input.dummy_queue_midi_msgs(stopped_event)
                fx_input.dummy_request_data(1)
                process(1)
                verify_eq(fx_input.dummy_dequeue_midi_msgs(), [], null, true)

                track.control_widget.monitor = true
                testcase.wait_updated(session.backend)
                let monitored_event = [
                    { 'time': 0, 'data': [0x90, 72, 100] }
                ]
                external_input.dummy_queue_midi_msgs(monitored_event)
                fx_input.dummy_request_data(1)
                process(1)
                verify_eq(fx_input.dummy_dequeue_midi_msgs(), monitored_event, null, true)

                track.control_widget.monitor = false
                testcase.wait_updated(session.backend)
                let muted_event = [
                    { 'time': 0, 'data': [0x90, 73, 100] }
                ]
                external_input.dummy_queue_midi_msgs(muted_event)
                fx_input.dummy_request_data(1)
                process(1)
                verify_eq(fx_input.dummy_dequeue_midi_msgs(), [], null, true)
            }

            test_fns: ({
                // Purpose: Carla Rack must follow every dry/wet track activation mode and stop inactive output.
                // Use case: Rack processing starts only while monitoring, recording, or reprocessing requires it.
                'test_carla_rack_activation_modes': () => {
                    check_backend()
                    verify_activation_modes(tracks[0])
                },

                // Purpose: Carla Patchbay must follow every dry/wet track activation mode and stop inactive output.
                // Use case: Patchbay processing starts only while monitoring, recording, or reprocessing requires it.
                'test_carla_patchbay_activation_modes': () => {
                    check_backend()
                    verify_activation_modes(tracks[1])
                },

                // Purpose: Carla Patchbay 16x must follow every mode and stop output while inactive.
                // Use case: A 16-channel external-effect layout obeys the same lifecycle as stereo tracks.
                'test_carla_patchbay_16_activation_modes': () => {
                    check_backend()
                    verify_activation_modes(tracks[2])
                },

                // Purpose: Inactive Carla Rack MIDI input must be gated and resume only while active.
                // Use case: A synth hosted in Rack must not receive live notes from an unmonitored track.
                // Failure: Expected active FX input [{time:0,data:[0x90,72,100]}]; observed [].
                // Carla MIDI input capture may bypass or miss the session's internal MIDI propagation.
                'test_carla_rack_midi_activation_gating': () => {
                    check_backend()
                    verify_midi_gating(tracks[0])
                },

                // Purpose: Inactive Carla Patchbay MIDI input must be gated and resume only while active.
                // Use case: A Patchbay synth graph must not receive notes from an unmonitored track.
                // Failure: Expected active FX input [{time:0,data:[0x90,72,100]}]; observed [].
                // Carla MIDI input capture may bypass or miss the session's internal MIDI propagation.
                'test_carla_patchbay_midi_activation_gating': () => {
                    check_backend()
                    verify_midi_gating(tracks[1])
                },

                // Purpose: Inactive Carla Patchbay 16x MIDI input must be gated and resume while active.
                // Use case: A large Patchbay graph must not receive notes from an unmonitored track.
                // Failure: Expected active FX input [{time:0,data:[0x90,72,100]}]; observed [].
                // Carla MIDI input capture may bypass or miss the session's internal MIDI propagation.
                'test_carla_patchbay_16_midi_activation_gating': () => {
                    check_backend()
                    verify_midi_gating(tracks[2])
                }
            })
        }
    }
}
