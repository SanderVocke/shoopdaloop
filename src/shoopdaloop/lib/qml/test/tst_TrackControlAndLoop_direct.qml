import QtQuick 6.3
import QtTest 1.0
import ShoopDaLoop.PythonBackend

import './testDeepEqual.js' as TestDeepEqual
import '../../generated/types.js' as Types
import '../../generate_session.js' as GenerateSession
import './testfilename.js' as TestFilename
import '..'

AppRegistries {
    Session {
        id: session

        anchors.fill: parent
        initial_descriptor: {
            let base = GenerateSession.generate_default_session(app_metadata.version_string, 1)
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
            base.tracks.push(track)
            testcase.logger.debug(() => ("session descriptor: " + JSON.stringify(base, null, 2)))
            return base
        }

        ShoopSessionTestCase {
            id: testcase
            name: 'TrackControlAndLoop_direct'
            filename : TestFilename.test_filename()
            session: session

            property var tut : session.tracks[1]
            property var lut : session.tracks[1].loops[0] // LoopWidget
            
            function tut_control() {
                return session.get_track_control_widget(1)
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

            function initTestCase() {
                session.backend.dummy_enter_controlled_mode()
                verify_true(input_port_1)
                verify_true(input_port_2)
                verify_true(output_port_1)
                verify_true(output_port_2)
                verify_true(midi_input_port)
                verify_true(midi_output_port)
                reset()
            }

            function reset_track(track_idx) {
                let t = session.tracks[track_idx]
                let c = session.get_track_control_widget(track_idx)
                c.input_balance = 0.0
                c.output_balance = 0.0
                c.volume_dB = 0.0
                c.input_volume_dB = 0.0
                c.monitor = false
                c.mute = false
            }

            function reset_loop(loopwidget) {
                loopwidget.transition(Types.LoopMode.Stopped, 0, false)
                loopwidget.clear(0)
                session.backend.dummy_wait_process()
            }

            function reset() {
                reset_track(0)
                reset_track(1)
                reset_loop(lut)
                session.backend.dummy_wait_process()
            }

                'test_audio_direct_record_no_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = false
                    tut_control().mute = false
                    lut.transition(Types.LoopMode.Recording, 0, false)
                    testcase.wait_updated(session.backend)

                    input_port_1.dummy_queue_data([1, 2, 3, 4])
                    input_port_2.dummy_queue_data([4, 3, 2, 1])
                    output_port_1.dummy_request_data(4)
                    output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_wait_process()

                    let out1 = output_port_1.dummy_dequeue_data(4)
                    let out2 = output_port_2.dummy_dequeue_data(4)
                    let chans = lut.get_audio_output_channels()
                    let loop1 = chans[0].get_data()
                    let loop2 = chans[1].get_data()

                    verify_eq(out1, [0, 0, 0, 0])
                    verify_eq(out2, [0, 0, 0, 0])
                    verify_eq(loop1, [1, 2, 3, 4])
                    verify_eq(loop2, [4, 3, 2, 1])

                })
            }

                'test_midi_direct_record_no_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = false
                    tut_control().mute = false
                    lut.transition(Types.LoopMode.Recording, 0, false)
                    testcase.wait_updated(session.backend)

                    let input = [
                        { 'time': 0, 'data': [0x90, 100, 100] },
                        { 'time': 3, 'data': [0x90, 50,  50]  },
                        { 'time': 4, 'data': [0x90, 10,  10]  }
                    ]
                    let expect_loop = [
                        { 'time': 0, 'data': [0x90, 100, 100] },
                        { 'time': 3, 'data': [0x90, 50,  50]  },
                    ]
                    let expect_out = []
                    let chan = lut.get_midi_output_channels()[0]

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    midi_input_port.dummy_queue_msgs(input)
                    midi_output_port.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.dummy_wait_process()
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.dummy_wait_process()

                    let out = midi_output_port.dummy_dequeue_data()
                    let chan_data = chan.get_data()

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    verify_eq(out, expect_out, true)
                    verify_eq(chan_data, expect_loop, true)
                })
            }

            test_fns: ({
                'test_audio_direct_record_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = true
                    tut_control().mute = false
                    lut.transition(Types.LoopMode.Recording, 0, false)
                    testcase.wait_updated(session.backend)

                    input_port_1.dummy_queue_data([1, 2, 3, 4])
                    input_port_2.dummy_queue_data([4, 3, 2, 1])
                    output_port_1.dummy_request_data(4)
                    output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_wait_process()

                    let out1 = output_port_1.dummy_dequeue_data(4)
                    let out2 = output_port_2.dummy_dequeue_data(4)
                    let chans = lut.get_audio_output_channels()
                    let loop1 = chans[0].get_data()
                    let loop2 = chans[1].get_data()

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
                    lut.transition(Types.LoopMode.Recording, 0, false)
                    testcase.wait_updated(session.backend)

                    let input = [
                        { 'time': 0, 'data': [0x90, 100, 100] },
                        { 'time': 3, 'data': [0x90, 50,  50]  },
                        { 'time': 4, 'data': [0x90, 10,  10]  }
                    ]
                    let expect_loop = [
                        { 'time': 0, 'data': [0x90, 100, 100] },
                        { 'time': 3, 'data': [0x90, 50,  50]  },
                    ]
                    let expect_out = expect_loop
                    let chan = lut.get_midi_output_channels()[0]

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    midi_input_port.dummy_queue_msgs(input)
                    midi_output_port.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.dummy_wait_process()
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.dummy_wait_process()

                    let out = midi_output_port.dummy_dequeue_data()
                    let chan_data = chan.get_data()

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    verify_eq(out, expect_out, true)
                    verify_eq(chan_data, expect_loop, true)
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
                    lut.transition(Types.LoopMode.Playing, 0, false)
                    testcase.wait_updated(session.backend)

                    input_port_1.dummy_queue_data([1, 2, 3, 4])
                    input_port_2.dummy_queue_data([4, 3, 2, 1])
                    output_port_1.dummy_request_data(4)
                    output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_wait_process()

                    let out1 = output_port_1.dummy_dequeue_data(4)
                    let out2 = output_port_2.dummy_dequeue_data(4)
                    let loop1 = chans[0].get_data()
                    let loop2 = chans[1].get_data()

                    verify_eq(out1, [5, 6, 7, 8])
                    verify_eq(out2, [8, 7, 6, 5])
                    verify_eq(loop1, [5, 6, 7, 8])
                    verify_eq(loop2, [8, 7, 6, 5])

                },

                'test_midi_direct_play_no_monitor': () => {
                    let input = [
                        { 'time': 0, 'data': [0x90, 100, 100] },
                        { 'time': 3, 'data': [0x90, 50,  50]  }
                    ]
                    let loop = [
                        { 'time': 1, 'data': [0x90, 20, 20] },
                        { 'time': 2, 'data': [0x90, 10, 10] },
                    ]
                    let expect_out = loop

                    check_backend()
                    reset()
                    tut_control().monitor = false
                    tut_control().mute = false
                    let chan = lut.get_midi_output_channels()[0]
                    chan.load_data(loop)
                    lut.set_length(4)
                    lut.transition(Types.LoopMode.Playing, 0, false)
                    testcase.wait_updated(session.backend)

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    midi_input_port.dummy_queue_msgs(input)
                    midi_output_port.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.dummy_wait_process()
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.dummy_wait_process()

                    let out = midi_output_port.dummy_dequeue_data()
                    let chan_data = chan.get_data()

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    verify_eq(out, expect_out, true)
                    verify_eq(chan_data, loop, true)
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
                    lut.transition(Types.LoopMode.Playing, 0, false)
                    testcase.wait_updated(session.backend)

                    input_port_1.dummy_queue_data([1, 2, 3, 4])
                    input_port_2.dummy_queue_data([4, 3, 2, 1])
                    output_port_1.dummy_request_data(4)
                    output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_wait_process()

                    let out1 = output_port_1.dummy_dequeue_data(4)
                    let out2 = output_port_2.dummy_dequeue_data(4)
                    let loop1 = chans[0].get_data()
                    let loop2 = chans[1].get_data()

                    verify_eq(out1, [6, 8, 10, 12])
                    verify_eq(out2, [12, 10, 8, 6])
                    verify_eq(loop1, [5, 6, 7, 8])
                    verify_eq(loop2, [8, 7, 6, 5])

                },

                'test_midi_direct_play_monitor': () => {
                    let input = [
                        { 'time': 0, 'data': [0x90, 100, 100] },
                        { 'time': 3, 'data': [0x90, 50,  50]  }
                    ]
                    let loop = [
                        { 'time': 1, 'data': [0x90, 20, 20] },
                        { 'time': 2, 'data': [0x90, 10, 10] },
                    ]
                    let expect_out = [
                        { 'time': 0, 'data': [0x90, 100, 100] },
                        { 'time': 1, 'data': [0x90, 20, 20]   },
                        { 'time': 2, 'data': [0x90, 10, 10]   },
                        { 'time': 3, 'data': [0x90, 50,  50]  }
                    ]

                    check_backend()
                    reset()
                    tut_control().monitor = true
                    tut_control().mute = false
                    let chan = lut.get_midi_output_channels()[0]
                    chan.load_data(loop)
                    lut.set_length(4)
                    lut.transition(Types.LoopMode.Playing, 0, false)
                    testcase.wait_updated(session.backend)

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    midi_input_port.dummy_queue_msgs(input)
                    midi_output_port.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.dummy_wait_process()
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.dummy_wait_process()

                    let out = midi_output_port.dummy_dequeue_data()
                    let chan_data = chan.get_data()

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    verify_eq(out, expect_out, true)
                    verify_eq(chan_data, loop, true)
                },
            })
        }
    }
}