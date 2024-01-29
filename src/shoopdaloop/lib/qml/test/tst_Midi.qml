import QtQuick 6.3
import QtTest 1.0
import ShoopDaLoop.PythonBackend

import './testDeepEqual.js' as TestDeepEqual
import ShoopConstants
import '../../generate_session.js' as GenerateSession
import './testfilename.js' as TestFilename
import '../../midi.js' as Midi
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
                0, // direct Audio
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
            property var syncloop : session.sync_track.loops[0] // LoopWidget
            
            function tut_control() {
                return tut.control_widget
            }

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
                verify_true(midi_input_port)
                verify_true(midi_output_port)
                reset()
            }

            function reset_track(track) {
                let t = track
                let c = t.control_widget
                c.monitor = false
                c.mute = false
            }

            function reset_loop(loopwidget) {
                loopwidget.transition(ShoopConstants.LoopMode.Stopped, 0, false)
                loopwidget.clear(0)
                session.backend.wait_process()
            }

            function reset() {
                reset_track(session.sync_track)
                reset_track(session.main_tracks[0])
                reset_loop(lut)
                reset_loop(syncloop)
                session.backend.wait_process()
            }

            test_fns: ({
                'test_midi_direct_record_then_play': () => {
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
                    let chan = lut.get_midi_output_channels()[0]

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    midi_input_port.dummy_queue_msgs(input)

                    // Process 6 frames (record)
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.wait_process()
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.wait_process()

                    // Data should now be in channel. Switch to playback.
                    lut.transition(ShoopConstants.LoopMode.Playing, 0, false)

                    midi_output_port.dummy_request_data(6)

                    // Process 6 frames (play back)
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.wait_process()
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.wait_process()

                    let out = midi_output_port.dummy_dequeue_data()

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    verify_eq(out, input, true)
                },

                'test_midi_direct_prerecord_then_play': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = false
                    tut_control().mute = false

                    // Set sync loop so that it will trigger in 2 frames from now
                    syncloop.set_length(4)
                    syncloop.transition(ShoopConstants.LoopMode.Playing, 0, false)
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.wait_process()

                    // Set main loop to record (will pre-record, then transition @ sync)
                    lut.transition(ShoopConstants.LoopMode.Recording, 0, true)
                    testcase.wait_updated(session.backend)

                    let input = [
                        { 'time': 0, 'data': [0x90, 100, 100] }, // during pre-reord
                        { 'time': 3, 'data': [0x90, 50,  50]  }, // during record
                        { 'time': 4, 'data': [0x90, 10,  10]  }  // during record
                    ]
                    let chan = lut.get_midi_output_channels()[0]

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    midi_input_port.dummy_queue_msgs(input)

                    // Process 6 frames (prerecord then record)
                    session.backend.dummy_request_controlled_frames(6)
                    session.backend.wait_process()

                    // Data should now be in channel. Switch to playback.
                    lut.transition(ShoopConstants.LoopMode.Playing, 0, false)

                    midi_output_port.dummy_request_data(4)

                    // Process 6 frames (play back)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.wait_process()

                    let out = midi_output_port.dummy_dequeue_data()

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    let expect_output = [
                        { 'time': 1, 'data': [0x90, 50,  50]  }, // from input[1]
                        { 'time': 2, 'data': [0x90, 10,  10]  }  // from input[2]
                    ]

                    verify_eq(chan.get_data(), input, true)
                    verify_eq(out, expect_output, true)
                }
            }),

            'test_input_port_note_tracking': () => {
                check_backend()
                reset()
                tut_control().monitor = false
                tut_control().mute = false

                midi_input_port.dummy_queue_msgs([
                    {'time': 0, 'data': Midi.create_noteOn(0, 100, 100) },
                    {'time': 1, 'data': Midi.create_noteOn(0, 50,  50)  },
                    {'time': 2, 'data': Midi.create_noteOff(0, 100, 90) },
                    {'time': 3, 'data': Midi.create_noteOff(0, 50,  40) }
                ])

                verify_eq(midi_input_port.n_input_notes_active, 0)
                verify_eq(midi_input_port.n_input_events, 0)
                
                session.backend.dummy_request_controlled_frames(1)
                session.backend.wait_process()
                testcase.wait_updated(session.backend)

                verify_eq(midi_input_port.n_input_notes_active, 1)
                verify_eq(midi_input_port.n_input_events, 1)

                session.backend.dummy_request_controlled_frames(1)
                session.backend.wait_process()
                testcase.wait_updated(session.backend)

                verify_eq(midi_input_port.n_input_notes_active, 2)
                verify_eq(midi_input_port.n_input_events, 2)

                session.backend.dummy_request_controlled_frames(1)
                session.backend.wait_process()
                testcase.wait_updated(session.backend)

                verify_eq(midi_input_port.n_input_notes_active, 1)
                verify_eq(midi_input_port.n_input_events, 3)

                session.backend.dummy_request_controlled_frames(1)
                session.backend.wait_process()
                testcase.wait_updated(session.backend)

                verify_eq(midi_input_port.n_input_notes_active, 0)
                verify_eq(midi_input_port.n_input_events, 4)

                midi_input_port.dummy_clear_queues()
                midi_output_port.dummy_clear_queues()
            },
        }
    }
}