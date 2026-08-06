import QtQuick 6.6

import ShoopDaLoop.Rust
import '../js/generate_session.js' as GenerateSession
import './testfilename.js' as TestFilename
import '..'

ShoopTestFile {
    TestSession {
        id: session
        backend_type: ShoopRustConstants.AudioDriverType.Dummy
        anchors.fill: parent
        initial_descriptor: {
            let track = GenerateSession.generate_default_track(
                "persist",
                1,
                "persist",
                false,
                "persist",
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
            name: 'Session_save_load_drywet_external'
            filename: TestFilename.test_filename()
            session: session

            property var track: session.main_tracks[0]

            RegistryLookup {
                id: audio_input_lookup
                registry: AppRegistries.objects_registry
                key: "persist_audio_dry_in_1"
            }
            property alias audio_input: audio_input_lookup.object

            RegistryLookup {
                id: audio_send_lookup
                registry: AppRegistries.objects_registry
                key: "persist_audio_dry_send_1"
            }
            property alias audio_send: audio_send_lookup.object

            RegistryLookup {
                id: midi_input_lookup
                registry: AppRegistries.objects_registry
                key: "persist_dry_midi_in"
            }
            property alias midi_input: midi_input_lookup.object

            RegistryLookup {
                id: midi_send_lookup
                registry: AppRegistries.objects_registry
                key: "persist_dry_midi_send"
            }
            property alias midi_send: midi_send_lookup.object

            RegistryLookup {
                id: wet_return_lookup
                registry: AppRegistries.objects_registry
                key: "persist_audio_wet_return_1"
            }
            property alias wet_return: wet_return_lookup.object

            RegistryLookup {
                id: wet_output_lookup
                registry: AppRegistries.objects_registry
                key: "persist_audio_wet_out_1"
            }
            property alias wet_output: wet_output_lookup.object

            testcase_init_fn: () => {
                session.backend.dummy_enter_controlled_mode()
                testcase.wait_controlled_mode(session.backend)
                verify_objects()
                testcase.wait_updated(session.backend)
            }

            function verify_objects() {
                verify_true(track)
                verify_true(audio_input)
                verify_true(audio_send)
                verify_true(midi_input)
                verify_true(midi_send)
                verify_true(wet_return)
                verify_true(wet_output)
            }

            function verify_monitor_state(monitored) {
                verify_eq(track.control_widget.monitor, monitored)
                verify_eq(audio_input.passthrough_muted, !monitored)
                verify_eq(midi_input.passthrough_muted, !monitored)
                verify_eq(wet_return.passthrough_muted, !monitored)
            }

            function verify_routing(monitored) {
                let audio = [1, 2, 3, 4]
                let returned = [10, 20, 30, 40]
                let midi = [
                    { 'time': 0, 'data': [0x90, 70, 100] },
                    { 'time': 3, 'data': [0x80, 70, 0] }
                ]
                midi_input.dummy_clear_queues()
                midi_send.dummy_clear_queues()
                audio_input.dummy_queue_audio_data(audio)
                midi_input.dummy_queue_midi_msgs(midi)
                wet_return.dummy_queue_audio_data(returned)
                audio_send.dummy_request_data(4)
                midi_send.dummy_request_data(4)
                wet_output.dummy_request_data(4)
                session.backend.dummy_request_controlled_frames(4)
                session.backend.dummy_run_requested_frames()
                testcase.wait_updated(session.backend)

                verify_eq(audio_send.dummy_dequeue_audio_data(4), monitored ? audio : [0, 0, 0, 0])
                verify_eq(midi_send.dummy_dequeue_midi_msgs(), monitored ? midi : [], null, true)
                verify_eq(wet_output.dummy_dequeue_audio_data(4), monitored ? returned : [0, 0, 0, 0])
            }

            function save_then_mutate_and_load(saved_monitor, mutated_monitor) {
                track.control_widget.monitor = saved_monitor
                testcase.wait_updated(session.backend)
                let filename = ShoopRustFileIO.generate_temporary_filename() + '.shl'
                session.save_session(filename)
                testcase.wait_session_io_done()

                track.control_widget.monitor = mutated_monitor
                testcase.wait_updated(session.backend)
                session.load_session(filename)
                testcase.wait_session_loaded(session)
                testcase.wait_session_io_done()
                testcase.wait_updated(session.backend)
                ShoopRustFileIO.delete_file(filename)
                verify_objects()
            }

            test_fns: ({
                // Purpose: A fresh explicit external track must start with every live path effectively muted.
                // Use case: Adding an external-effects track must not immediately pass input or return audio.
                'test_fresh_external_track_defaults_to_monitoring_off': () => {
                    check_backend()
                    verify_monitor_state(false)
                    verify_routing(false)
                },

                // Purpose: Saving monitoring off must restore control and backend passthrough mute state.
                // Use case: A silent external track remains silent after closing and reopening a session.
                'test_save_load_external_monitoring_off': () => {
                    check_backend()
                    save_then_mutate_and_load(false, true)
                    verify_monitor_state(false)
                    verify_routing(false)
                },

                // Purpose: Saving monitoring on must restore control and backend passthrough routing state.
                // Use case: A live external processor path is immediately usable after reopening a session.
                'test_save_load_external_monitoring_on': () => {
                    check_backend()
                    save_then_mutate_and_load(true, false)
                    verify_monitor_state(true)
                    verify_routing(true)
                }
            })
        }
    }
}
