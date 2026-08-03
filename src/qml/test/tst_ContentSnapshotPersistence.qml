import QtQuick 6.6
import QtQuick.Controls 6.6

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
                "snapshot_persistence",
                1,
                "snapshot_persistence",
                false,
                "snapshot_persistence",
                0,
                0,
                2,
                false,
                true,
                false,
                undefined)
            return GenerateSession.generate_default_session(
                global_args.version_string, null, true, 1, 1, [track])
        }

        ShoopSessionTestCase {
            id: testcase
            name: 'ContentSnapshotPersistence'
            filename: TestFilename.test_filename()
            session: session

            property var loop_widget: session.main_tracks[0].loops[0]

            testcase_init_fn: () => {
                session.backend.dummy_enter_controlled_mode()
                testcase.wait_controlled_mode(session.backend)
                loop_widget.transition(ShoopRustConstants.LoopMode.Stopped, ShoopRustConstants.DontWaitForSync, ShoopRustConstants.DontAlignToSyncImmediately)
                testcase.wait_updated(session.backend)
                loop_widget.clear(0)
                session.backend.wait_process()
            }

            test_fns: ({
                'test_session_save_rejects_recording_content': () => {
                    check_backend()
                    let filename = ShoopRustFileIO.generate_temporary_filename() + '.shl'
                    loop_widget.transition(ShoopRustConstants.LoopMode.Recording, ShoopRustConstants.DontWaitForSync, ShoopRustConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(1)
                    session.backend.dummy_run_requested_frames()
                    testcase.wait_updated(session.backend)

                    session.save_session(filename)
                    testcase.wait_session_io_done()
                    verify_true(!ShoopRustFileIO.exists(filename))

                    loop_widget.transition(ShoopRustConstants.LoopMode.Stopped, ShoopRustConstants.DontWaitForSync, ShoopRustConstants.DontAlignToSyncImmediately)
                    testcase.wait_updated(session.backend)
                    session.backend.dummy_request_controlled_frames(1)
                    session.backend.dummy_run_requested_frames()
                    testcase.wait_updated(session.backend)
                },

                'test_audio_export_rejects_pending_load_and_clear_then_retries': () => {
                    check_backend()
                    let chan = loop_widget.get_audio_output_channels()[0]
                    let filename = ShoopRustFileIO.generate_temporary_filename() + '.wav'

                    chan.load_data([1, 2, 3, 4])
                    verify_true(!ShoopRustFileIO.save_channels_to_soundfile(
                                     filename,
                                     session.backend.sample_rate,
                                     [chan]))
                    verify_true(!ShoopRustFileIO.exists(filename))

                    session.backend.wait_process()
                    verify_eq(chan.get_data(), [1, 2, 3, 4])
                    verify_true(ShoopRustFileIO.save_channels_to_soundfile(
                                    filename,
                                    session.backend.sample_rate,
                                    [chan]))
                    verify_true(ShoopRustFileIO.exists(filename))
                    verify_true(ShoopRustFileIO.delete_file(filename))

                    chan.clear(0)
                    verify_true(!ShoopRustFileIO.save_channels_to_soundfile(
                                     filename,
                                     session.backend.sample_rate,
                                     [chan]))
                    verify_true(!ShoopRustFileIO.exists(filename))
                },

                'test_midi_export_rejects_pending_load_then_retries': () => {
                    check_backend()
                    let chan = loop_widget.get_midi_output_channels()[0]
                    let filename = ShoopRustFileIO.generate_temporary_filename() + '.smf'

                    chan.load_midi_data([
                        { 'time': 1, 'data': [0x90, 60, 100] }
                    ])
                    verify_true(!ShoopRustFileIO.save_channel_to_midi(
                                     filename,
                                     session.backend.sample_rate,
                                     chan))
                    verify_true(!ShoopRustFileIO.exists(filename))

                    session.backend.wait_process()
                    verify_eq(chan.get_recorded_midi_msgs().length, 1)
                    verify_true(ShoopRustFileIO.save_channel_to_midi(
                                    filename,
                                    session.backend.sample_rate,
                                    chan))
                    verify_true(ShoopRustFileIO.exists(filename))
                    verify_true(ShoopRustFileIO.delete_file(filename))
                }
            })
        }
    }
}
