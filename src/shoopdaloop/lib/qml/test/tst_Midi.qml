import QtQuick 6.6
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
            name: 'Midi'
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
                loopwidget.transition(ShoopConstants.LoopMode.Stopped, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                testcase.wait_updated(session.backend)
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
                'midi_record_then_play': () => {
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
                    let chan = lut.get_midi_output_channels()[0]

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    midi_input_port.dummy_queue_msgs(input)

                    // Process 6 frames (record)
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.dummy_run_requested_frames()
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    // Data should now be in channel. Switch to playback.
                    lut.transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)

                    midi_output_port.dummy_request_data(6)

                    // Process 6 frames (play back)
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.dummy_run_requested_frames()
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    let out = midi_output_port.dummy_dequeue_data()

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    verify_eq(out, input, null, true)
                },

                'midi_prerecord_then_play': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = false
                    tut_control().mute = false

                    // Set sync loop so that it will trigger in 2 frames from now
                    syncloop.set_length(4)
                    syncloop.transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.dummy_run_requested_frames()

                    // Set main loop to record (will pre-record, then transition @ sync)
                    lut.transition(ShoopConstants.LoopMode.Recording, 0, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)

                    let input = [
                        { 'time': 0, 'data': [0x80, 100, 100] }, // during pre-reord
                        { 'time': 3, 'data': [0x80, 50,  50]  }, // during record
                        { 'time': 4, 'data': [0x80, 10,  10]  }  // during record
                    ]
                    let chan = lut.get_midi_output_channels()[0]

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    midi_input_port.dummy_queue_msgs(input)

                    // Process 6 frames (prerecord then record)
                    session.backend.dummy_request_controlled_frames(6)
                    session.backend.dummy_run_requested_frames()

                    // Data should now be in channel. Switch to playback.
                    lut.transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)

                    midi_output_port.dummy_request_data(4)

                    // Process 6 frames (play back)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    let out = midi_output_port.dummy_dequeue_data()

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    let expect_output = [
                        { 'time': 1, 'data': [0x80, 50,  50]  }, // from input[1]
                        { 'time': 2, 'data': [0x80, 10,  10]  }  // from input[2]
                    ]

                    verify_eq(chan.get_recorded_midi_msgs(), input, null, true)
                    verify_eq(out, expect_output, null, true)
                },

                'midi_prerecord_then_play_stateful': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = false
                    tut_control().mute = false

                    // Set sync loop so that it will trigger in 20 frames from now
                    syncloop.set_length(20)
                    syncloop.transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(20)
                    session.backend.dummy_run_requested_frames()

                    // Set main loop to record (will pre-record, then transition @ sync)
                    lut.transition(ShoopConstants.LoopMode.Recording, 0, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)

                    let input = [
                        { 'time': 10, 'data': Midi.create_noteOn(0, 100, 100) }, // during pre-reord
                        { 'time': 26, 'data': Midi.create_noteOff(0, 100, 90) }, // during record
                        { 'time': 28,'data': Midi.create_noteOn(0, 120, 80) },
                        { 'time': 32,'data': Midi.create_noteOff(0, 120, 70) }
                    ]
                    let chan = lut.get_midi_output_channels()[0]

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    midi_input_port.dummy_queue_msgs(input)

                    // Process 40 frames (prerecord then record)
                    session.backend.dummy_request_controlled_frames(40)
                    session.backend.dummy_run_requested_frames()

                    // Data should now be in channel. Switch to playback.
                    lut.transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)

                    midi_output_port.dummy_request_data(40)

                    // Process 40 frames (play back twice)
                    session.backend.dummy_request_controlled_frames(40)
                    session.backend.dummy_run_requested_frames()
                    let out = midi_output_port.dummy_dequeue_data()

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    let expect_output = [
                        { 'time': 0, 'data':  input[0]['data']  }, // from state (no pre-play was done)
                        { 'time': 6, 'data':  input[1]['data']  },
                        { 'time': 8, 'data':  input[2]['data']  },
                        { 'time': 12, 'data': input[3]['data']  },
                        { 'time': 20, 'data':  Midi.create_all_sound_off(0) }, // end-of-loop
                        { 'time': 20, 'data':  input[0]['data']  }, // from state (no pre-play was done)
                        { 'time': 26, 'data':  input[1]['data']  },
                        { 'time': 28, 'data':  input[2]['data']  },
                        { 'time': 32, 'data': input[3]['data']  },
                    ]

                    verify_eq(chan.get_recorded_midi_msgs(), input, null, true)
                    verify_eq(out, expect_output, null, true)

                    lut.transition(ShoopConstants.LoopMode.Stopped, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(20)
                    session.backend.dummy_run_requested_frames()
                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    // When saving this to disk and re-loading, the state stuff should still work
                    // and play back in the exact same way. That means the state itself should be
                    // somehow saved. Test here that it works.
                    var filename = file_io.generate_temporary_filename() + '.smf'
                    file_io.save_channel_to_midi(filename, session.backend.get_sample_rate(), chan)
                    chan.clear()
                    testcase.wait_updated(session.backend)
                    testcase.wait_updated(session.backend)
                    verify_eq(chan.get_all_midi_data(), [], null, true)
                    file_io.load_midi_to_channels(
                                filename,
                                session.backend.get_sample_rate(),
                                [chan],
                                0,
                                20,
                                false)

                    lut.transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    midi_output_port.dummy_request_data(40)
                    session.backend.dummy_request_controlled_frames(40)
                    session.backend.dummy_run_requested_frames()
                    out = midi_output_port.dummy_dequeue_data()

                    // Verify same as before
                    verify_eq(chan.get_recorded_midi_msgs(), input, null, true)
                    verify_eq(out, expect_output, null, true)
                },

                'midi_record_then_play_stateful': () => {
                    check_backend()
                    reset()
                    tut_control().monitor = false
                    tut_control().mute = false

                    let input = [
                        { 'time': 1, 'data': [0x90, 100, 100] }, // before record
                        { 'time': 4, 'data': [0x80, 100,  50] }, // during record
                    ]
                    let chan = lut.get_midi_output_channels()[0]

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    midi_input_port.dummy_queue_msgs(input)

                    // Process 6 frames (nothing, then record)
                    session.backend.dummy_request_controlled_frames(2)
                    session.backend.dummy_run_requested_frames()
                    lut.transition(ShoopConstants.LoopMode.Recording, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()

                    // NoteOn should now be in state, NoteOff should be in the
                    // recording. Start playback.
                    lut.transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)

                    midi_output_port.dummy_request_data(4)

                    // Process 4 frames (play back)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()
                    let out = midi_output_port.dummy_dequeue_data()

                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    let expect = [
                        { 'time': 0, 'data': [0x90, 100, 100] },
                        { 'time': 2, 'data': [0x80, 100,  50] },
                    ]
                    verify_eq(out, expect, null, true)

                    // Now, save the channel to disk, load it in again and verify that
                    // playback behavior is still the same.

                    // First stop for a while
                    lut.transition(ShoopConstants.LoopMode.Stopped, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(20)
                    session.backend.dummy_run_requested_frames()
                    midi_input_port.dummy_clear_queues()
                    midi_output_port.dummy_clear_queues()

                    // Save and re-load
                    var filename = file_io.generate_temporary_filename() + '.smf'
                    file_io.save_channel_to_midi(filename, session.backend.get_sample_rate(), chan)
                    chan.clear()
                    testcase.wait_updated(session.backend)
                    testcase.wait_updated(session.backend)
                    verify_eq(chan.get_all_midi_data(), [], null, true)
                    file_io.load_midi_to_channels(
                                filename,
                                session.backend.get_sample_rate(),
                                [chan],
                                0,
                                0,
                                false)

                    lut.transition(ShoopConstants.LoopMode.Playing, ShoopConstants.DontWaitForSync, ShoopConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    midi_output_port.dummy_request_data(4)
                    session.backend.dummy_request_controlled_frames(4)
                    session.backend.dummy_run_requested_frames()
                    out = midi_output_port.dummy_dequeue_data()

                    // Verify same as before
                    verify_eq(out, expect, null, true)
                },
            })
        }
    }
}