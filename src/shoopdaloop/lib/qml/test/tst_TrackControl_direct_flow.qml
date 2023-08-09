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

        property var tut : session.tracks[1]

        function initTestCase() {
            session.backend.dummy_enter_controlled_mode()
        }

        function test_direct_passthrough() {
            start_test_fn('test_direct_passthrough')
            check_backend()


            end_test_fn('test_direct_passthrough')
        }
    }
}