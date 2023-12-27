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
            let base = GenerateSession.generate_default_session(app_metadata.version_string, null, 1)
            let track = GenerateSession.generate_default_track(
                "tut",
                1,
                "tut",
                false,
                "tut",
                2,
                2,
                0,
                true, //dry midi
                false,
                false,
                "test2x2x1"
                )
            base.tracks.push(track)
            testcase.logger.debug(() => ("session descriptor: " + JSON.stringify(base, null, 2)))
            return base
        }

        ShoopSessionTestCase {
            id: testcase
            name: 'TrackControlAndLoop_drywet'
            filename : TestFilename.test_filename()
            session: session

            property var tut : session.tracks[1]
            property var lut : session.tracks[1].loops[0] // LoopWidget

            function wet_channels() {
                if (!lut) return []
                var r = lut.get_audio_channels().filter(c => c.obj_id.match(/.*_wet_.*/))
                r.sort((a,b) => a.obj_id.localeCompare(b.obj_id))
                return r
            }

            function dry_channels() {
                if (!lut) return []
                var r = lut.get_audio_channels().filter(c => c.obj_id.match(/.*_dry_.*/))
                r.sort((a,b) => a.obj_id.localeCompare(b.obj_id))
                return r
            }

            function midi_channel() {
                return lut.get_midi_channels()[0]
            }
            
            function tut_control() {
                return session.get_track_control_widget(1)
            }

            RegistryLookup {
                id: lookup_input_port_1
                registry: registries.objects_registry
                key: "tut_audio_dry_in_1"
            }
            property alias input_port_1: lookup_input_port_1.object

            RegistryLookup {
                id: lookup_input_port_2
                registry: registries.objects_registry
                key: "tut_audio_dry_in_2"
            }
            property alias input_port_2: lookup_input_port_2.object

            RegistryLookup {
                id: lookup_output_port_1
                registry: registries.objects_registry
                key: "tut_audio_wet_out_1"
            }
            property alias output_port_1: lookup_output_port_1.object

            RegistryLookup {
                id: lookup_output_port_2
                registry: registries.objects_registry
                key: "tut_audio_wet_out_2"
            }
            property alias output_port_2: lookup_output_port_2.object

            RegistryLookup {
                id: lookup_midi_input_port
                registry: registries.objects_registry
                key: "tut_dry_midi_in"
            }
            property alias midi_input_port: lookup_midi_input_port.object

            RegistryLookup {
                id: lookup_fx
                registry: registries.objects_registry
                key: "tut_fx_chain"
            }
            property alias fx: lookup_fx.object

            testcase_init_fn: () =>  {
                session.backend.dummy_enter_controlled_mode()
                verify_true(input_port_1)
                verify_true(input_port_2)
                verify_true(output_port_1)
                verify_true(output_port_2)
                verify_true(midi_input_port)
                verify_true(fx)
                verify_true(lut)
                verify_true(tut)
                lut.create_backend_loop()
                verify_true(midi_channel())
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
                loopwidget.transition(ShoopConstants.LoopMode.Stopped, 0, false)
                loopwidget.clear(0)
                session.backend.wait_process()
            }

            function reset() {
                reset_track(0)
                reset_track(1)
                reset_loop(lut)
                session.backend.wait_process()
            }

            function synthed_value_for(midi_msg) {
                return midi_msg.data[2] / 255.0
            }

            function elems_add(a, b) {
                return a.map((value, index) => value + b[index]);
            }

            test_fns: ({
                'test_drywet_audio_record_no_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = false
                    tut_control().mute = false
                    lut.transition(ShoopConstants.LoopMode.Recording, 0, false)
                    testcase.wait_updated(session.backend)

                    input_port_1.dummy_queue_data([2, 4, 6, 8])
                    input_port_2.dummy_queue_data([8, 6, 4, 2])
                    output_port_1.dummy_request_data(4)
                    output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    
                    testcase.wait_updated(session.backend)

                    let out1 = output_port_1.dummy_dequeue_data(4)
                    let out2 = output_port_2.dummy_dequeue_data(4)
                    let dry1 = dry_channels()[0].get_data()
                    let dry2 = dry_channels()[1].get_data()
                    let wet1 = wet_channels()[0].get_data()
                    let wet2 = wet_channels()[1].get_data()
                    verify_true(fx.active)

                    verify_eq(out1, [0, 0, 0, 0])
                    verify_eq(out2, [0, 0, 0, 0])
                    verify_eq(dry1, [2, 4, 6, 8])
                    verify_eq(dry2, [8, 6, 4, 2])
                    verify_eq(wet1, [1, 2, 3, 4])
                    verify_eq(wet2, [4, 3, 2, 1])

                },

                'test_drywet_midi_record_no_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = false
                    tut_control().mute = false
                    lut.transition(ShoopConstants.LoopMode.Recording, 0, false)
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
                    let expect_wet = [
                        synthed_value_for(expect_loop[0]),
                        0,
                        0,
                        synthed_value_for(expect_loop[1])
                    ]
                    let expect_dry = [0, 0, 0, 0]
                    let expect_out = [0, 0, 0, 0]

                    midi_input_port.dummy_clear_queues()

                    midi_input_port.dummy_queue_msgs(input)
                    output_port_1.dummy_request_data(4)
                    output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.wait_process()
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.wait_process()

                    let out1 = output_port_1.dummy_dequeue_data(4)
                    let out2 = output_port_2.dummy_dequeue_data(4)
                    let dry1 = dry_channels()[0].get_data()
                    let dry2 = dry_channels()[1].get_data()
                    let wet1 = wet_channels()[0].get_data()
                    let wet2 = wet_channels()[1].get_data()
                    let midi = midi_channel().get_data()

                    midi_input_port.dummy_clear_queues()

                    verify_true(fx.active)
                    verify_approx(dry1, expect_dry)
                    verify_approx(dry2, expect_dry)
                    verify_approx(wet1, expect_wet)
                    verify_approx(wet2, expect_wet)
                    verify_approx(out1, expect_out)
                    verify_approx(out2, expect_out)
                    verify_eq(midi, expect_loop, true)
                },

                'test_drywet_audio_record_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = true
                    tut_control().mute = false
                    lut.transition(ShoopConstants.LoopMode.Recording, 0, false)
                    testcase.wait_updated(session.backend)

                    input_port_1.dummy_queue_data([2, 4, 6, 8])
                    input_port_2.dummy_queue_data([8, 6, 4, 2])
                    output_port_1.dummy_request_data(4)
                    output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.wait_process()

                    let out1 = output_port_1.dummy_dequeue_data(4)
                    let out2 = output_port_2.dummy_dequeue_data(4)
                    let dry1 = dry_channels()[0].get_data()
                    let dry2 = dry_channels()[1].get_data()
                    let wet1 = wet_channels()[0].get_data()
                    let wet2 = wet_channels()[1].get_data()
                    verify_true(fx.active)

                    verify_eq(out1, [1, 2, 3, 4])
                    verify_eq(out2, [4, 3, 2, 1])
                    verify_eq(dry1, [2, 4, 6, 8])
                    verify_eq(dry2, [8, 6, 4, 2])
                    verify_eq(wet1, [1, 2, 3, 4])
                    verify_eq(wet2, [4, 3, 2, 1])

                },

                'test_drywet_midi_record_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = true
                    tut_control().mute = false
                    lut.transition(ShoopConstants.LoopMode.Recording, 0, false)
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
                    let expect_out = [
                        synthed_value_for(expect_loop[0]),
                        0,
                        0,
                        synthed_value_for(expect_loop[1])
                    ]
                    let expect_dry = [0, 0, 0, 0]
                    let expect_wet = expect_out

                    midi_input_port.dummy_clear_queues()

                    midi_input_port.dummy_queue_msgs(input)
                    output_port_1.dummy_request_data(4)
                    output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.wait_process()
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.wait_process()

                    let out1 = output_port_1.dummy_dequeue_data(4)
                    let out2 = output_port_2.dummy_dequeue_data(4)
                    let dry1 = dry_channels()[0].get_data()
                    let dry2 = dry_channels()[1].get_data()
                    let wet1 = wet_channels()[0].get_data()
                    let wet2 = wet_channels()[1].get_data()
                    let midi = midi_channel().get_data()

                    midi_input_port.dummy_clear_queues()

                    verify_true(fx.active)
                    verify_approx(dry1, expect_dry)
                    verify_approx(dry2, expect_dry)
                    verify_approx(wet1, expect_wet)
                    verify_approx(wet2, expect_wet)
                    verify_approx(out1, expect_out)
                    verify_approx(out2, expect_out)
                    verify_eq(midi, expect_loop, true)
                },

                'test_drywet_audio_play_no_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = false
                    tut_control().mute = false
                    dry_channels()[0].load_data([50, 60, 70, 80])
                    dry_channels()[1].load_data([80, 70, 60, 50])
                    wet_channels()[0].load_data([5, 6, 7, 8])
                    wet_channels()[1].load_data([8, 7, 6, 5])
                    lut.set_length(4)
                    lut.transition(ShoopConstants.LoopMode.Playing, 0, false)
                    testcase.wait_updated(session.backend)

                    input_port_1.dummy_queue_data([1, 2, 3, 4])
                    input_port_2.dummy_queue_data([4, 3, 2, 1])
                    output_port_1.dummy_request_data(4)
                    output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.wait_process()

                    let out1 = output_port_1.dummy_dequeue_data(4)
                    let out2 = output_port_2.dummy_dequeue_data(4)
                    let dry1 = dry_channels()[0].get_data()
                    let dry2 = dry_channels()[1].get_data()
                    let wet1 = wet_channels()[0].get_data()
                    let wet2 = wet_channels()[1].get_data()
                    verify_true(!fx.active)

                    verify_eq(out1, [5, 6, 7, 8])
                    verify_eq(out2, [8, 7, 6, 5])
                    verify_eq(dry1, [50, 60, 70, 80])
                    verify_eq(dry2, [80, 70, 60, 50])
                    verify_eq(wet1, [5, 6, 7, 8])
                    verify_eq(wet2, [8, 7, 6, 5])

                },

                'test_drywet_midi_play_no_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = false
                    tut_control().mute = false
                    dry_channels()[0].load_data([50, 60, 70, 80])
                    dry_channels()[1].load_data([80, 70, 60, 50])
                    wet_channels()[0].load_data([5, 6, 7, 8])
                    wet_channels()[1].load_data([8, 7, 6, 5])
                    let midichan = [
                        { 'time': 1, 'data': [0x90, 70,  70]  }
                    ]
                    midi_channel().load_data(midichan)
                    lut.set_length(4)
                    lut.transition(ShoopConstants.LoopMode.Playing, 0, false)
                    testcase.wait_updated(session.backend)

                    let input = [
                        { 'time': 0, 'data': [0x90, 100, 100] },
                        { 'time': 3, 'data': [0x90, 50,  50]  },
                        { 'time': 4, 'data': [0x90, 10,  10]  }
                    ]
                    let expect_loop = []

                    midi_input_port.dummy_clear_queues()

                    midi_input_port.dummy_queue_msgs(input)
                    output_port_1.dummy_request_data(4)
                    output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.wait_process()
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.wait_process()

                    let out1 = output_port_1.dummy_dequeue_data(4)
                    let out2 = output_port_2.dummy_dequeue_data(4)
                    let dry1 = dry_channels()[0].get_data()
                    let dry2 = dry_channels()[1].get_data()
                    let wet1 = wet_channels()[0].get_data()
                    let wet2 = wet_channels()[1].get_data()
                    let midi = midi_channel().get_data()

                    midi_input_port.dummy_clear_queues()

                    verify_true(!fx.active)
                    verify_eq(out1, [5, 6, 7, 8])
                    verify_eq(out2, [8, 7, 6, 5])
                    verify_eq(dry1, [50, 60, 70, 80])
                    verify_eq(dry2, [80, 70, 60, 50])
                    verify_eq(wet1, [5, 6, 7, 8])
                    verify_eq(wet2, [8, 7, 6, 5])
                    verify_eq(midi, midichan, true)
                },

                'test_drywet_audio_play_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = true
                    tut_control().mute = false
                    dry_channels()[0].load_data([50, 60, 70, 80])
                    dry_channels()[1].load_data([80, 70, 60, 50])
                    wet_channels()[0].load_data([5, 6, 7, 8])
                    wet_channels()[1].load_data([8, 7, 6, 5])
                    lut.set_length(4)
                    lut.transition(ShoopConstants.LoopMode.Playing, 0, false)
                    testcase.wait_updated(session.backend)

                    input_port_1.dummy_queue_data([2, 4, 6, 8])
                    input_port_2.dummy_queue_data([8, 6, 4, 2])
                    output_port_1.dummy_request_data(4)
                    output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.wait_process()

                    let out1 = output_port_1.dummy_dequeue_data(4)
                    let out2 = output_port_2.dummy_dequeue_data(4)
                    let dry1 = dry_channels()[0].get_data()
                    let dry2 = dry_channels()[1].get_data()
                    let wet1 = wet_channels()[0].get_data()
                    let wet2 = wet_channels()[1].get_data()

                    verify_eq(out1, [6, 8, 10, 12])
                    verify_eq(out2, [12, 10, 8, 6])
                    verify_eq(dry1, [50, 60, 70, 80])
                    verify_eq(dry2, [80, 70, 60, 50])
                    verify_eq(wet1, [5, 6, 7, 8])
                    verify_eq(wet2, [8, 7, 6, 5])

                    verify_true(fx.active)

                },

                'test_drywet_midi_play_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = true
                    tut_control().mute = false
                    dry_channels()[0].load_data([50, 60, 70, 80])
                    dry_channels()[1].load_data([80, 70, 60, 50])
                    wet_channels()[0].load_data([5, 6, 7, 8])
                    wet_channels()[1].load_data([8, 7, 6, 5])
                    let midichan = [
                        { 'time': 1, 'data': [0x90, 70,  70]  }
                    ]
                    midi_channel().load_data(midichan)
                    lut.set_length(4)
                    lut.transition(ShoopConstants.LoopMode.Playing, 0, false)
                    testcase.wait_updated(session.backend)

                    let input = [
                        { 'time': 0, 'data': [0x90, 100, 100] },
                        { 'time': 3, 'data': [0x90, 50,  50]  },
                        { 'time': 4, 'data': [0x90, 10,  10]  }
                    ]
                    let synthed = [
                        synthed_value_for(input[0]),
                        0,
                        0,
                        synthed_value_for(input[1])
                    ]

                    midi_input_port.dummy_clear_queues()

                    midi_input_port.dummy_queue_msgs(input)
                    output_port_1.dummy_request_data(4)
                    output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.wait_process()
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.wait_process()

                    let out1 = output_port_1.dummy_dequeue_data(4)
                    let out2 = output_port_2.dummy_dequeue_data(4)
                    let dry1 = dry_channels()[0].get_data()
                    let dry2 = dry_channels()[1].get_data()
                    let wet1 = wet_channels()[0].get_data()
                    let wet2 = wet_channels()[1].get_data()
                    let midi = midi_channel().get_data()

                    midi_input_port.dummy_clear_queues()

                    verify_true(fx.active)
                    verify_approx(out1, elems_add([5, 6, 7, 8], synthed))
                    verify_approx(out2, elems_add([8, 7, 6, 5], synthed))
                    verify_eq(dry1, [50, 60, 70, 80])
                    verify_eq(dry2, [80, 70, 60, 50])
                    verify_eq(wet1, [5, 6, 7, 8])
                    verify_eq(wet2, [8, 7, 6, 5])
                    verify_eq(midi, midichan, true)
                },

                'test_drywet_audio_playdry_no_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = false
                    tut_control().mute = false
                    dry_channels()[0].load_data([50, 60, 70, 80])
                    dry_channels()[1].load_data([80, 70, 60, 50])
                    wet_channels()[0].load_data([5, 6, 7, 8])
                    wet_channels()[1].load_data([8, 7, 6, 5])
                    lut.set_length(4)
                    lut.transition(ShoopConstants.LoopMode.PlayingDryThroughWet, 0, false)
                    testcase.wait_updated(session.backend)

                    input_port_1.dummy_queue_data([1, 2, 3, 4])
                    input_port_2.dummy_queue_data([4, 3, 2, 1])
                    output_port_1.dummy_request_data(4)
                    output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.wait_process()
                    testcase.wait_updated(session.backend)

                    let l = session.get_track_control_widget(1).control_logic

                    let out1 = output_port_1.dummy_dequeue_data(4)
                    let out2 = output_port_2.dummy_dequeue_data(4)
                    let dry1 = dry_channels()[0].get_data()
                    let dry2 = dry_channels()[1].get_data()
                    let wet1 = wet_channels()[0].get_data()
                    let wet2 = wet_channels()[1].get_data()

                    verify_true(fx.active)
                    verify_eq(out1, [25, 30, 35, 40])
                    verify_eq(out2, [40, 35, 30, 25])
                    verify_eq(dry1, [50, 60, 70, 80])
                    verify_eq(dry2, [80, 70, 60, 50])
                    verify_eq(wet1, [5, 6, 7, 8])
                    verify_eq(wet2, [8, 7, 6, 5])

                },

                'test_drywet_midi_playdry_no_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = false
                    tut_control().mute = false
                    dry_channels()[0].load_data([50, 60, 70, 80])
                    dry_channels()[1].load_data([80, 70, 60, 50])
                    wet_channels()[0].load_data([5, 6, 7, 8])
                    wet_channels()[1].load_data([8, 7, 6, 5])
                    let midichan = [
                        { 'time': 1, 'data': [0x90, 70,  70]  }
                    ]
                    midi_channel().load_data(midichan)
                    lut.set_length(4)
                    lut.transition(ShoopConstants.LoopMode.PlayingDryThroughWet, 0, false)
                    testcase.wait_updated(session.backend)

                    let input = [
                        { 'time': 0, 'data': [0x90, 100, 100] },
                        { 'time': 3, 'data': [0x90, 50,  50]  },
                        { 'time': 4, 'data': [0x90, 10,  10]  }
                    ]
                    let synthed = [
                        synthed_value_for(input[0]),
                        0,
                        0,
                        synthed_value_for(input[1])
                    ]
                    let synthed_chan = [
                        0,
                        synthed_value_for(midichan[0]),
                        0,
                        0
                    ]

                    midi_input_port.dummy_clear_queues()

                    midi_input_port.dummy_queue_msgs(input)
                    output_port_1.dummy_request_data(4)
                    output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.wait_process()
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.wait_process()
                    testcase.wait_updated(session.backend)

                    let out1 = output_port_1.dummy_dequeue_data(4)
                    let out2 = output_port_2.dummy_dequeue_data(4)
                    let dry1 = dry_channels()[0].get_data()
                    let dry2 = dry_channels()[1].get_data()
                    let wet1 = wet_channels()[0].get_data()
                    let wet2 = wet_channels()[1].get_data()
                    let midi = midi_channel().get_data()

                    midi_input_port.dummy_clear_queues()

                    verify_true(fx.active)
                    verify_approx(out1, elems_add(synthed_chan, [50, 60, 70, 80]))
                    verify_approx(out2, elems_add(synthed_chan, [80, 70, 60, 50]))
                    verify_eq(dry1, [50, 60, 70, 80])
                    verify_eq(dry2, [80, 70, 60, 50])
                    verify_eq(wet1, [5, 6, 7, 8])
                    verify_eq(wet2, [8, 7, 6, 5])
                    verify_eq(midi, midichan, true)
                },

                'test_drywet_audio_playdry_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = true
                    tut_control().mute = false
                    dry_channels()[0].load_data([50, 60, 70, 80])
                    dry_channels()[1].load_data([80, 70, 60, 50])
                    wet_channels()[0].load_data([5, 6, 7, 8])
                    wet_channels()[1].load_data([8, 7, 6, 5])
                    lut.set_length(4)
                    lut.transition(ShoopConstants.LoopMode.PlayingDryThroughWet, 0, false)
                    testcase.wait_updated(session.backend)

                    input_port_1.dummy_queue_data([2, 4, 6, 8])
                    input_port_2.dummy_queue_data([8, 6, 4, 2])
                    output_port_1.dummy_request_data(4)
                    output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.wait_process()

                    let out1 = output_port_1.dummy_dequeue_data(4)
                    let out2 = output_port_2.dummy_dequeue_data(4)
                    let dry1 = dry_channels()[0].get_data()
                    let dry2 = dry_channels()[1].get_data()
                    let wet1 = wet_channels()[0].get_data()
                    let wet2 = wet_channels()[1].get_data()

                    verify_true(fx.active)
                    verify_eq(out1, [26, 32, 38, 44])
                    verify_eq(out2, [44, 38, 32, 26])
                    verify_eq(dry1, [50, 60, 70, 80])
                    verify_eq(dry2, [80, 70, 60, 50])
                    verify_eq(wet1, [5, 6, 7, 8])
                    verify_eq(wet2, [8, 7, 6, 5])

                },

                'test_drywet_midi_playdry_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = true
                    tut_control().mute = false
                    dry_channels()[0].load_data([50, 60, 70, 80])
                    dry_channels()[1].load_data([80, 70, 60, 50])
                    wet_channels()[0].load_data([5, 6, 7, 8])
                    wet_channels()[1].load_data([8, 7, 6, 5])
                    let midichan = [
                        { 'time': 1, 'data': [0x90, 70,  70]  }
                    ]
                    midi_channel().load_data(midichan)
                    lut.set_length(4)
                    lut.transition(ShoopConstants.LoopMode.PlayingDryThroughWet, 0, false)
                    testcase.wait_updated(session.backend)

                    let input = [
                        { 'time': 0, 'data': [0x90, 100, 100] },
                        { 'time': 3, 'data': [0x90, 50,  50]  },
                        { 'time': 4, 'data': [0x90, 10,  10]  }
                    ]
                    let synthed = [
                        synthed_value_for(input[0]),
                        0,
                        0,
                        synthed_value_for(input[1])
                    ]
                    let synthed_chan = [
                        0,
                        synthed_value_for(midichan[0]),
                        0,
                        0
                    ]
                    let synthed_both = elems_add(synthed, synthed_chan)

                    midi_input_port.dummy_clear_queues()

                    midi_input_port.dummy_queue_msgs(input)
                    output_port_1.dummy_request_data(4)
                    output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.wait_process()
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.wait_process()
                    testcase.wait_updated(session.backend)

                    let out1 = output_port_1.dummy_dequeue_data(4)
                    let out2 = output_port_2.dummy_dequeue_data(4)
                    let dry1 = dry_channels()[0].get_data()
                    let dry2 = dry_channels()[1].get_data()
                    let wet1 = wet_channels()[0].get_data()
                    let wet2 = wet_channels()[1].get_data()
                    let midi = midi_channel().get_data()

                    midi_input_port.dummy_clear_queues()

                    verify_true(fx.active)
                    verify_approx(out1, elems_add(synthed_both, [50, 60, 70, 80]))
                    verify_approx(out2, elems_add(synthed_both, [80, 70, 60, 50]))
                    verify_eq(dry1, [50, 60, 70, 80])
                    verify_eq(dry2, [80, 70, 60, 50])
                    verify_eq(wet1, [5, 6, 7, 8])
                    verify_eq(wet2, [8, 7, 6, 5])
                    verify_eq(midi, midichan, true)
                },

                'test_drywet_audio_rerecord_no_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = false
                    tut_control().mute = false
                    dry_channels()[0].load_data([50, 60, 70, 80])
                    dry_channels()[1].load_data([80, 70, 60, 50])
                    wet_channels()[0].load_data([5, 6, 7, 8])
                    wet_channels()[1].load_data([8, 7, 6, 5])
                    lut.set_length(4)
                    lut.transition(ShoopConstants.LoopMode.RecordingDryIntoWet, 0, false)
                    testcase.wait_updated(session.backend)

                    input_port_1.dummy_queue_data([1, 2, 3, 4])
                    input_port_2.dummy_queue_data([4, 3, 2, 1])
                    output_port_1.dummy_request_data(4)
                    output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.wait_process()
                    testcase.wait_updated(session.backend)

                    let l = session.get_track_control_widget(1).control_logic

                    let out1 = output_port_1.dummy_dequeue_data(4)
                    let out2 = output_port_2.dummy_dequeue_data(4)
                    let dry1 = dry_channels()[0].get_data()
                    let dry2 = dry_channels()[1].get_data()
                    let wet1 = wet_channels()[0].get_data()
                    let wet2 = wet_channels()[1].get_data()

                    verify_true(fx.active)
                    verify_eq(out1, [25, 30, 35, 40])
                    verify_eq(out2, [40, 35, 30, 25])
                    verify_eq(dry1, [50, 60, 70, 80])
                    verify_eq(dry2, [80, 70, 60, 50])
                    verify_eq(wet1, [25, 30, 35, 40])
                    verify_eq(wet2, [40, 35, 30, 25])

                },

                'test_drywet_midi_rerecord_no_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = false
                    tut_control().mute = false
                    dry_channels()[0].load_data([50, 60, 70, 80])
                    dry_channels()[1].load_data([80, 70, 60, 50])
                    wet_channels()[0].load_data([5, 6, 7, 8])
                    wet_channels()[1].load_data([8, 7, 6, 5])
                    let midichan = [
                        { 'time': 1, 'data': [0x90, 70,  70]  }
                    ]
                    midi_channel().load_data(midichan)
                    lut.set_length(4)
                    lut.transition(ShoopConstants.LoopMode.RecordingDryIntoWet, 0, false)
                    testcase.wait_updated(session.backend)

                    let input = [
                        { 'time': 0, 'data': [0x90, 100, 100] },
                        { 'time': 3, 'data': [0x90, 50,  50]  },
                        { 'time': 4, 'data': [0x90, 10,  10]  }
                    ]
                    let synthed = [
                        synthed_value_for(input[0]),
                        0,
                        0,
                        synthed_value_for(input[1])
                    ]
                    let synthed_chan = [
                        0,
                        synthed_value_for(midichan[0]),
                        0,
                        0
                    ]

                    midi_input_port.dummy_clear_queues()

                    midi_input_port.dummy_queue_msgs(input)
                    output_port_1.dummy_request_data(4)
                    output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.wait_process()
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.wait_process()
                    testcase.wait_updated(session.backend)

                    let out1 = output_port_1.dummy_dequeue_data(4)
                    let out2 = output_port_2.dummy_dequeue_data(4)
                    let dry1 = dry_channels()[0].get_data()
                    let dry2 = dry_channels()[1].get_data()
                    let wet1 = wet_channels()[0].get_data()
                    let wet2 = wet_channels()[1].get_data()
                    let midi = midi_channel().get_data()

                    midi_input_port.dummy_clear_queues()

                    verify_true(fx.active)
                    verify_approx(out1, elems_add(synthed_chan, [50, 60, 70, 80]))
                    verify_approx(out2, elems_add(synthed_chan, [80, 70, 60, 50]))
                    verify_eq(dry1, [50, 60, 70, 80])
                    verify_eq(dry2, [80, 70, 60, 50])
                    verify_approx(wet1, elems_add(synthed_chan, [50, 60, 70, 80]))
                    verify_approx(wet2, elems_add(synthed_chan, [80, 70, 60, 50]))
                    verify_eq(midi, midichan, true)
                },

                'test_drywet_audio_rerecord_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = true
                    tut_control().mute = false
                    dry_channels()[0].load_data([50, 60, 70, 80])
                    dry_channels()[1].load_data([80, 70, 60, 50])
                    wet_channels()[0].load_data([5, 6, 7, 8])
                    wet_channels()[1].load_data([8, 7, 6, 5])
                    lut.set_length(4)
                    lut.transition(ShoopConstants.LoopMode.RecordingDryIntoWet, 0, false)
                    testcase.wait_updated(session.backend)

                    input_port_1.dummy_queue_data([1, 2, 3, 4])
                    input_port_2.dummy_queue_data([4, 3, 2, 1])
                    output_port_1.dummy_request_data(4)
                    output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.wait_process()
                    testcase.wait_updated(session.backend)

                    let out1 = output_port_1.dummy_dequeue_data(4)
                    let out2 = output_port_2.dummy_dequeue_data(4)
                    let dry1 = dry_channels()[0].get_data()
                    let dry2 = dry_channels()[1].get_data()
                    let wet1 = wet_channels()[0].get_data()
                    let wet2 = wet_channels()[1].get_data()

                    verify_true(fx.active)
                    verify_eq(out1, [25, 30, 35, 40])
                    verify_eq(out2, [40, 35, 30, 25])
                    verify_eq(dry1, [50, 60, 70, 80])
                    verify_eq(dry2, [80, 70, 60, 50])
                    verify_eq(wet1, [25, 30, 35, 40])
                    verify_eq(wet2, [40, 35, 30, 25])

                },

                'test_drywet_midi_rerecord_monitor': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = true
                    tut_control().mute = false
                    dry_channels()[0].load_data([50, 60, 70, 80])
                    dry_channels()[1].load_data([80, 70, 60, 50])
                    wet_channels()[0].load_data([5, 6, 7, 8])
                    wet_channels()[1].load_data([8, 7, 6, 5])
                    let midichan = [
                        { 'time': 1, 'data': [0x90, 70,  70]  }
                    ]
                    midi_channel().load_data(midichan)
                    lut.set_length(4)
                    lut.transition(ShoopConstants.LoopMode.RecordingDryIntoWet, 0, false)
                    testcase.wait_updated(session.backend)

                    let input = [
                        { 'time': 0, 'data': [0x90, 100, 100] },
                        { 'time': 3, 'data': [0x90, 50,  50]  },
                        { 'time': 4, 'data': [0x90, 10,  10]  }
                    ]
                    let synthed = [
                        synthed_value_for(input[0]),
                        0,
                        0,
                        synthed_value_for(input[1])
                    ]
                    let synthed_chan = [
                        0,
                        synthed_value_for(midichan[0]),
                        0,
                        0
                    ]
                    let synthed_both = elems_add(synthed, synthed_chan)

                    midi_input_port.dummy_clear_queues()

                    midi_input_port.dummy_queue_msgs(input)
                    output_port_1.dummy_request_data(4)
                    output_port_2.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.wait_process()
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.wait_process()
                    testcase.wait_updated(session.backend)

                    let out1 = output_port_1.dummy_dequeue_data(4)
                    let out2 = output_port_2.dummy_dequeue_data(4)
                    let dry1 = dry_channels()[0].get_data()
                    let dry2 = dry_channels()[1].get_data()
                    let wet1 = wet_channels()[0].get_data()
                    let wet2 = wet_channels()[1].get_data()
                    let midi = midi_channel().get_data()

                    midi_input_port.dummy_clear_queues()

                    verify_true(fx.active)
                    verify_approx(out1, elems_add(synthed_both, [50, 60, 70, 80]))
                    verify_approx(out2, elems_add(synthed_both, [80, 70, 60, 50]))
                    verify_eq(dry1, [50, 60, 70, 80])
                    verify_eq(dry2, [80, 70, 60, 50])
                    verify_approx(wet1, elems_add(synthed_both, [50, 60, 70, 80]))
                    verify_approx(wet2, elems_add(synthed_both, [80, 70, 60, 50]))
                    verify_eq(midi, midichan, true)
                },
            })
        }
    }
}