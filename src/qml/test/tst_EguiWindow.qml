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
            const stereoTrack = GenerateSession.generate_default_track(
                "egui_stereo",
                1,
                "egui_stereo",
                false,
                "egui_stereo"
            )
            const monoTrack = GenerateSession.generate_default_track(
                "egui_mono",
                1,
                "egui_mono",
                false,
                "egui_mono",
                0,
                0,
                1,
                false,
                false
            )
            const midiTrack = GenerateSession.generate_default_track(
                "egui_midi",
                1,
                "egui_midi",
                false,
                "egui_midi",
                0,
                0,
                0,
                false,
                true
            )
            return GenerateSession.generate_default_session(
                global_args.version_string,
                null,
                true,
                1,
                1,
                [stereoTrack, monoTrack, midiTrack]
            )
        }

        Component {
            id: eguiWindowFactory
            EguiWindow {}
        }

        ShoopSessionTestCase {
            name: 'EguiWindow'
            filename: TestFilename.test_filename()
            session: session

            test_fns: ({
                'test_window_and_track_bridges_initialize': () => {
                    const window = eguiWindowFactory.createObject(session, {
                        tracks: Array.from(session.tracks),
                        visible: true
                    })
                    verify_true(window)
                    wait_condition(() => window.initialized, 2000, "egui window did not initialize")
                    const track = session.main_tracks[0]
                    const trackIndex = session.tracks.indexOf(track)
                    const control = track.control_widget
                    verify_true(control)

                    window.handleTrackNameChanged(trackIndex, "Renamed from egui")
                    window.handleTrackOutputGainChanged(trackIndex, -6.0)
                    window.handleTrackOutputBalanceChanged(trackIndex, 0.25)
                    window.handleTrackOutputMuteChanged(trackIndex, true)
                    window.handleTrackInputGainChanged(trackIndex, -3.0)
                    window.handleTrackInputBalanceChanged(trackIndex, -0.2)
                    window.handleTrackInputMonitoringChanged(trackIndex, true)

                    verify_eq(track.name, "Renamed from egui")
                    verify_approx(control.gain_dB, -6.0)
                    verify_approx(control.output_balance, 0.25)
                    verify_eq(control.mute, true)
                    verify_approx(control.input_gain_dB, -3.0)
                    verify_approx(control.input_balance, -0.2)
                    verify_eq(control.monitor, true)

                    track.name = "Renamed in QML"
                    wait(100)
                    window.close()
                }
            })
        }
    }
}
