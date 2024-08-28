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
            name: 'TrackControlAndLoop_direct'
            filename : TestFilename.test_filename()
            session: session

            property var tut : session.main_tracks[0]
            property var lut : session.main_tracks[0].loops[0] // LoopWidget
            
            function tut_control() {
                return tut.control_widget
            }

            RegistryLookup {
                id: lookup_input_port_1
                registry: registries.objects_registry
                key: "tut_direct_in_1"
            }
            property alias input_port_1: lookup_input_port_1.object

            RegistryLookup {
                id: lookup_input_port_2
                registry: registries.objects_registry
                key: "tut_direct_in_2"
            }
            property alias input_port_2: lookup_input_port_2.object

            RegistryLookup {
                id: lookup_output_port_1
                registry: registries.objects_registry
                key: "tut_direct_out_1"
            }
            property alias output_port_1: lookup_output_port_1.object

            RegistryLookup {
                id: lookup_output_port_2
                registry: registries.objects_registry
                key: "tut_direct_out_2"
            }
            property alias output_port_2: lookup_output_port_2.object

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
                verify_true(input_port_1)
                verify_true(input_port_2)
                verify_true(output_port_1)
                verify_true(output_port_2)
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

            function reset_loop(loopwidget) {
                loopwidget.transition(ShoopConstants.LoopMode.Stopped, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                testcase.wait_updated(session.backend)
                loopwidget.clear(0)
                session.backend.wait_process()
            }

            function reset() {
                reset_track(session.sync_track)
                reset_track(session.main_tracks[0])
                reset_loop(lut)
                session.backend.wait_process()
            }

            test_fns: ({
                'test_audio_direct_record_no_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = false
                    tut_control().mute = false
                    lut.transition(ShoopConstants.LoopMode.Recording, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)

                    input_port_1.dummy_queue_data([1, 2, 3, 4])
                    input_port_2.dummy_queue_data([4, 3, 2, 1])
                    output_port_1.dummy_request_data(4)
                    output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    let out1 = output_port_1.dummy_dequeue_data(4)
                    let out2 = output_port_2.dummy_dequeue_data(4)
                    let chans = lut.get_audio_output_channels()
                    let loop1 = chans[0].get_data_list()
                    let loop2 = chans[1].get_data_list()

                    verify_eq(out1, [0, 0, 0, 0])
                    verify_eq(out2, [0, 0, 0, 0])
                    verify_eq(loop1, [1, 2, 3, 4])
                    verify_eq(loop2, [4, 3, 2, 1])

                },

                'test_midi_direct_record_no_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = false
                    tut_control().mute = false
                    lut.transition(ShoopConstants.LoopMode.Recording, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)

                    let input = [
                        { 'time': 0, 'data': [0x80, 100, 100] },
                        { 'time': 3, 'data': [0x80, 50,  50]  },
                        { 'time': 4, 'data': [0x80, 10,  10]  }
                    ]
                    let expect_loop = [
                        { 'time': 0, 'data': [0x80, 100, 100] },
                        { 'time': 3, 'data': [0x80, 50,  50]  },
                    ]
                    let expect_out = []
                    let chan = lut.get_midi_output_channels()[0]

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    midi_input_port.dummy_queue_msgs(input)
                    midi_output_port.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.dummy_run_requested_frames()
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.dummy_run_requested_frames()

                    let out = midi_output_port.dummy_dequeue_data()
                    let chan_data = chan.get_recorded_midi_msgs()

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    verify_eq(out, expect_out, null, true)
                    verify_eq(chan_data, expect_loop, null, true)
                },

                'test_audio_direct_record_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = true
                    tut_control().mute = false
                    lut.transition(ShoopConstants.LoopMode.Recording, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)

                    input_port_1.dummy_queue_data([1, 2, 3, 4])
                    input_port_2.dummy_queue_data([4, 3, 2, 1])
                    output_port_1.dummy_request_data(4)
                    output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    let out1 = output_port_1.dummy_dequeue_data(4)
                    let out2 = output_port_2.dummy_dequeue_data(4)
                    let chans = lut.get_audio_output_channels()
                    let loop1 = chans[0].get_data_list()
                    let loop2 = chans[1].get_data_list()

                    verify_eq(out1, [1, 2, 3, 4])
                    verify_eq(out2, [4, 3, 2, 1])
                    verify_eq(loop1, [1, 2, 3, 4])
                    verify_eq(loop2, [4, 3, 2, 1])

                },

                'test_midi_direct_record_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = true
                    tut_control().mute = false
                    lut.transition(ShoopConstants.LoopMode.Recording, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)

                    let input = [
                        { 'time': 0, 'data': [0x80, 100, 100] },
                        { 'time': 3, 'data': [0x80, 50,  50]  },
                        { 'time': 4, 'data': [0x80, 10,  10]  }
                    ]
                    let expect_loop = [
                        { 'time': 0, 'data': [0x80, 100, 100] },
                        { 'time': 3, 'data': [0x80, 50,  50]  },
                    ]
                    let expect_out = expect_loop
                    let chan = lut.get_midi_output_channels()[0]

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    midi_input_port.dummy_queue_msgs(input)
                    midi_output_port.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.dummy_run_requested_frames()
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.dummy_run_requested_frames()

                    let out = midi_output_port.dummy_dequeue_data()
                    let chan_data = chan.get_recorded_midi_msgs()

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    verify_eq(out, expect_out, null, true)
                    verify_eq(chan_data, expect_loop, null, true)
                },

                'test_audio_direct_play_no_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = false
                    tut_control().mute = false
                    let chans = lut.get_audio_output_channels()
                    chans[0].load_data([5, 6, 7, 8])
                    chans[1].load_data([8, 7, 6, 5])
                    lut.set_length(4)
                    lut.transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)

                    input_port_1.dummy_queue_data([1, 2, 3, 4])
                    input_port_2.dummy_queue_data([4, 3, 2, 1])
                    output_port_1.dummy_request_data(4)
                    output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    let out1 = output_port_1.dummy_dequeue_data(4)
                    let out2 = output_port_2.dummy_dequeue_data(4)
                    let loop1 = chans[0].get_data_list()
                    let loop2 = chans[1].get_data_list()

                    verify_eq(out1, [5, 6, 7, 8])
                    verify_eq(out2, [8, 7, 6, 5])
                    verify_eq(loop1, [5, 6, 7, 8])
                    verify_eq(loop2, [8, 7, 6, 5])

                },

                'test_midi_direct_play_no_monitor': () => {
                    let input = [
                        { 'time': 0, 'data': [0x80, 100, 100] },
                        { 'time': 3, 'data': [0x80, 50,  50]  }
                    ]
                    let loop = [
                        { 'time': 1, 'data': [0x80, 20, 20] },
                        { 'time': 2, 'data': [0x80, 10, 10] },
                    ]
                    let expect_out = loop

                    check_backend()
                    reset()
                    tut_control().monitor = false
                    tut_control().mute = false
                    let chan = lut.get_midi_output_channels()[0]
                    chan.load_all_midi_data(loop)
                    lut.set_length(4)
                    lut.transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    midi_input_port.dummy_queue_msgs(input)
                    midi_output_port.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.dummy_run_requested_frames()
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.dummy_run_requested_frames()

                    let out = midi_output_port.dummy_dequeue_data()
                    let chan_data = chan.get_recorded_midi_msgs()

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    verify_eq(out, expect_out, null, true)
                    verify_eq(chan_data, loop, null, true)
                },

                'test_audio_direct_play_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = true
                    tut_control().mute = false
                    let chans = lut.get_audio_output_channels()
                    chans[0].load_data([5, 6, 7, 8])
                    chans[1].load_data([8, 7, 6, 5])
                    lut.set_length(4)
                    lut.transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)

                    input_port_1.dummy_queue_data([1, 2, 3, 4])
                    input_port_2.dummy_queue_data([4, 3, 2, 1])
                    output_port_1.dummy_request_data(4)
                    output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    let out1 = output_port_1.dummy_dequeue_data(4)
                    let out2 = output_port_2.dummy_dequeue_data(4)
                    let loop1 = chans[0].get_data_list()
                    let loop2 = chans[1].get_data_list()

                    verify_eq(out1, [6, 8, 10, 12])
                    verify_eq(out2, [12, 10, 8, 6])
                    verify_eq(loop1, [5, 6, 7, 8])
                    verify_eq(loop2, [8, 7, 6, 5])

                },

                'test_midi_direct_play_monitor': () => {
                    let input = [
                        { 'time': 0, 'data': [0x80, 100, 100] },
                        { 'time': 3, 'data': [0x80, 50,  50]  }
                    ]
                    let loop = [
                        { 'time': 1, 'data': [0x80, 20, 20] },
                        { 'time': 2, 'data': [0x80, 10, 10] },
                    ]
                    let expect_out = [
                        { 'time': 0, 'data': [0x80, 100, 100] },
                        { 'time': 1, 'data': [0x80, 20, 20]   },
                        { 'time': 2, 'data': [0x80, 10, 10]   },
                        { 'time': 3, 'data': [0x80, 50,  50]  }
                    ]

                    check_backend()
                    reset()
                    tut_control().monitor = true
                    tut_control().mute = false
                    let chan = lut.get_midi_output_channels()[0]
                    chan.load_all_midi_data(loop)
                    lut.set_length(4)
                    lut.transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    midi_input_port.dummy_queue_msgs(input)
                    midi_output_port.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.dummy_run_requested_frames()
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.dummy_run_requested_frames()

                    let out = midi_output_port.dummy_dequeue_data()
                    let chan_data = chan.get_recorded_midi_msgs()

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    verify_eq(out, expect_out, null, true)
                    verify_eq(chan_data, loop, null, true)
                },
            })
        }
    }
}