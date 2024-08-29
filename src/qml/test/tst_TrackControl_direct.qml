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
                "tut",
                0,
                0,
                2, // direct Audio
                false,
                true, // direct MIDI
                false,
                undefined
                )
            let base = GenerateSession.generate_default_session(app_metadata.version_string, null, true, 1, 1, [track])
            testcase.logger.debug(() => ("session descriptor: " + JSON.stringify(base, null, 2)))
            return base
        }

        ShoopSessionTestCase {
            id: testcase
            name: 'TrackControl_direct'
            filename : TestFilename.test_filename()
            session: session

            property var tut : session.main_tracks[0]
            
            function tut_control() {
                return tut.control_widget
            }

            RegistryLookup {
                id: lookup_audio_input_port_1
                registry: registries.objects_registry
                key: "tut_direct_in_1"
            }
            property alias audio_input_port_1: lookup_audio_input_port_1.object

            RegistryLookup {
                id: lookup_audio_input_port_2
                registry: registries.objects_registry
                key: "tut_direct_in_2"
            }
            property alias audio_input_port_2: lookup_audio_input_port_2.object

            RegistryLookup {
                id: lookup_audio_output_port_1
                registry: registries.objects_registry
                key: "tut_direct_out_1"
            }
            property alias audio_output_port_1: lookup_audio_output_port_1.object

            RegistryLookup {
                id: lookup_audio_output_port_2
                registry: registries.objects_registry
                key: "tut_direct_out_2"
            }
            property alias audio_output_port_2: lookup_audio_output_port_2.object

            RegistryLookup {
                id: lookup_midi_input_port
                registry: registries.objects_registry
                key: "tut_direct_midi_in"
            }
            property alias midi_input_port: lookup_midi_input_port.object

            RegistryLookup {
                id: lookup_midi_output_port
                registry: registries.objects_registry
                key: "tut_direct_midi_out"
            }
            property alias midi_output_port: lookup_midi_output_port.object

            testcase_init_fn: () =>  {
                session.backend.dummy_enter_controlled_mode()
                testcase.wait_controlled_mode(session.backend)
                verify_true(audio_input_port_1)
                verify_true(audio_input_port_2)
                verify_true(audio_output_port_1)
                verify_true(audio_output_port_2)
                verify_true(midi_input_port)
                verify_true(midi_output_port)
                reset()
            }

            function reset_track(track) {
                let t = track
                let c = t.control_widget
                c.input_balance = 0.0
                c.output_balance = 0.0
                c.gain_dB = 0.0
                c.input_gain_dB = 0.0
                c.monitor = false
                c.mute = false
            }

            function reset() {
                reset_track(session.sync_track)
                reset_track(session.main_tracks[0])
                session.backend.wait_process()
                testcase.wait_updated(session.backend)

                verify(audio_input_port_1.initialized)
                verify(audio_input_port_2.initialized)
                verify(audio_output_port_1.initialized)
                verify(audio_output_port_2.initialized)
                verify(midi_input_port.initialized)
                verify(midi_output_port.initialized)
            }

            test_fns: ({

                'test_direct_audio_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = true
                    tut_control().mute = false
                    testcase.wait_updated(session.backend)

                    audio_input_port_1.dummy_queue_data([1, 2, 3, 4])
                    audio_input_port_2.dummy_queue_data([4, 3, 2, 1])
                    audio_output_port_1.dummy_request_data(4)
                    audio_output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    let out1 = audio_output_port_1.dummy_dequeue_data(4)
                    let out2 = audio_output_port_2.dummy_dequeue_data(4)

                    verify_eq(out1.map(o => Math.round(o)), [1, 2, 3, 4])
                    verify_eq(out2.map(o => Math.round(o)), [4, 3, 2, 1])

                },

                'test_direct_midi_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = true
                    tut_control().mute = false
                    testcase.wait_updated(session.backend)

                    let msgs = [
                        { 'time': 0, 'data': [0x90, 100, 100] },
                        { 'time': 3, 'data': [0x90, 50,  50]  },
                        { 'time': 4, 'data': [0x90, 10,  10]  }
                    ]
                    let expect_out = [
                        { 'time': 0, 'data': [0x90, 100, 100] },
                        { 'time': 3, 'data': [0x90, 50,  50]  },
                        { 'time': 4, 'data': [0x90, 10,  10]  }
                    ]

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    midi_input_port.dummy_queue_msgs(msgs)
                    midi_output_port.dummy_request_data(8)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    let out = midi_output_port.dummy_dequeue_data()

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    verify_eq(out, expect_out, null, true)
                },

                'test_direct_audio_monitor_input_gain': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = true
                    tut_control().mute = false
                    tut_control().input_gain_dB = 6.0
                    testcase.wait_updated(session.backend)

                    audio_input_port_1.dummy_queue_data([1, 2, 3, 4])
                    audio_input_port_2.dummy_queue_data([4, 3, 2, 1])
                    audio_output_port_1.dummy_request_data(4)
                    audio_output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    let out1 = audio_output_port_1.dummy_dequeue_data(4)
                    let out2 = audio_output_port_2.dummy_dequeue_data(4)

                    verify_eq(out1.map(o => Math.round(o)), [2, 4, 6, 8])
                    verify_eq(out2.map(o => Math.round(o)), [8, 6, 4, 2])

                },

                'test_direct_audio_monitor_output_gain': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = true
                    tut_control().mute = false
                    tut_control().gain_dB = 6.0
                    testcase.wait_updated(session.backend)

                    audio_input_port_1.dummy_queue_data([1, 2, 3, 4])
                    audio_input_port_2.dummy_queue_data([4, 3, 2, 1])
                    audio_output_port_1.dummy_request_data(4)
                    audio_output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    let out1 = audio_output_port_1.dummy_dequeue_data(4)
                    let out2 = audio_output_port_2.dummy_dequeue_data(4)

                    verify_eq(out1.map(o => Math.round(o)), [2, 4, 6, 8])
                    verify_eq(out2.map(o => Math.round(o)), [8, 6, 4, 2])

                },

                'test_direct_audio_monitor_balance_left': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = true
                    tut_control().mute = false
                    tut_control().output_balance = -1.0
                    tut_control().gain_dB = 6.0
                    testcase.wait_updated(session.backend)

                    audio_input_port_1.dummy_queue_data([1, 2, 3, 4])
                    audio_input_port_2.dummy_queue_data([4, 3, 2, 1])
                    audio_output_port_1.dummy_request_data(4)
                    audio_output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    let out1 = audio_output_port_1.dummy_dequeue_data(4)
                    let out2 = audio_output_port_2.dummy_dequeue_data(4)

                    verify_eq(out1.map(o => Math.round(o)), [2, 4, 6, 8])
                    verify_eq(out2.map(o => Math.round(o)), [0, 0, 0, 0])

                },

                'test_direct_audio_monitor_output_balance_right': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = true
                    tut_control().mute = false
                    tut_control().output_balance = 1.0
                    tut_control().gain_dB = 6.0
                    testcase.wait_updated(session.backend)

                    audio_input_port_1.dummy_queue_data([1, 2, 3, 4])
                    audio_input_port_2.dummy_queue_data([4, 3, 2, 1])
                    audio_output_port_1.dummy_request_data(4)
                    audio_output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    let out1 = audio_output_port_1.dummy_dequeue_data(4)
                    let out2 = audio_output_port_2.dummy_dequeue_data(4)

                    verify_eq(out1.map(o => Math.round(o)), [0, 0, 0, 0])
                    verify_eq(out2.map(o => Math.round(o)), [8, 6, 4, 2])

                },

                'test_direct_audio_no_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = false
                    tut_control().mute = false
                    testcase.wait_updated(session.backend)

                    audio_input_port_1.dummy_queue_data([1, 2, 3, 4])
                    audio_input_port_2.dummy_queue_data([4, 3, 2, 1])
                    audio_output_port_1.dummy_request_data(4)
                    audio_output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    let out1 = audio_output_port_1.dummy_dequeue_data(4)
                    let out2 = audio_output_port_2.dummy_dequeue_data(4)

                    verify_eq(out1, [0, 0, 0, 0])
                    verify_eq(out2, [0, 0, 0, 0])

                },

                'test_direct_midi_no_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = false
                    tut_control().mute = false
                    testcase.wait_updated(session.backend)

                    let msgs = [
                        { 'time': 0, 'data': [0x90, 100, 100] },
                        { 'time': 3, 'data': [0x90, 50,  50]  },
                        { 'time': 4, 'data': [0x90, 10,  10]  }
                    ]

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    midi_input_port.dummy_queue_msgs(msgs)
                    midi_output_port.dummy_request_data(8)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    let out = midi_output_port.dummy_dequeue_data()

                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    let out2 = midi_output_port.dummy_dequeue_data()

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    verify_eq(out, [], null, true)
                    verify_eq(out2, [], null, true)
                },

                'test_direct_audio_monitor_mute': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = true
                    tut_control().mute = true
                    testcase.wait_updated(session.backend)

                    audio_input_port_1.dummy_queue_data([1, 2, 3, 4])
                    audio_input_port_2.dummy_queue_data([4, 3, 2, 1])
                    audio_output_port_1.dummy_request_data(4)
                    audio_output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    let out1 = audio_output_port_1.dummy_dequeue_data(4)
                    let out2 = audio_output_port_2.dummy_dequeue_data(4)

                    verify_eq(out1, [0, 0, 0, 0])
                    verify_eq(out2, [0, 0, 0, 0])

                },

                'test_direct_midi_monitor_mute': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = true
                    tut_control().mute = true
                    testcase.wait_updated(session.backend)

                    let msgs = [
                        { 'time': 0, 'data': [0x90, 100, 100] },
                        { 'time': 3, 'data': [0x90, 50,  50]  },
                        { 'time': 4, 'data': [0x90, 10,  10]  }
                    ]

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    midi_input_port.dummy_queue_msgs(msgs)
                    midi_output_port.dummy_request_data(8)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    let out = midi_output_port.dummy_dequeue_data()

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    verify_eq(out, [], null, true)
                },
            })
        }
    }
}