import QtQuick 6.3
import QtTest 1.0
import ShoopDaLoop.PythonBackend

import './testDeepEqual.js' as TestDeepEqual
import '../../generated/types.js' as Types
import '../../generate_session.js' as GenerateSession
import './testfilename.js' as TestFilename
import '..'

Session {
    id: session

    anchors.fill: parent
    initial_descriptor: {
        let base = GenerateSession.generate_default_session(app_metadata.version_string, 1)
        let track = GenerateSession.generate_default_track(
            name = "tut",
            n_loops = 1,
            id = "tut",
            first_loop_is_master = false,
            port_name_base = "tut",
            n_audio_dry = 0,
            n_audio_wet = 0,
            n_audio_direct = 2,
            have_midi_dry = false,
            have_midi_direct = false,
            have_drywet_jack_ports = false,
            drywet_carla_type = undefined
            )
        base.tracks.push(track)
        return base
    }

    ShoopSessionTestCase {
        id: testcase
        name: 'TrackControl_direct_flow'
        filename : TestFilename.test_filename()
        session: session

        function test_two_loops_cleared() {
            start_test_fn('test_two_loops_cleared')
            check_backend()

            clear()

            end_test_fn('test_two_loops_cleared')
        }

        function test_two_loops_master_record() {
            start_test_fn('test_two_loops_master_record')
            check_backend()

            master_loop().transition(Types.LoopMode.Recording, 0, true)
            testcase.wait(100)
            verify_eq(master_loop().mode, Types.LoopMode.Recording)
            verify_gt(master_loop().length, 0)
            verify_loop_cleared(other_loop())

            clear()
            end_test_fn('test_two_loops_master_record')
        }

        function test_two_loops_master_playback() {
            start_test_fn('test_two_loops_master_playback')
            check_backend()

            master_loop().set_length(48000)
            master_loop().transition(Types.LoopMode.Playing, 0, true)
            testcase.wait(100)
            verify_eq(master_loop().mode, Types.LoopMode.Playing)
            verify_eq(master_loop().length, 48000)
            verify_loop_cleared(other_loop())

            clear()
            end_test_fn('test_two_loops_master_playback')
        }
    }
}