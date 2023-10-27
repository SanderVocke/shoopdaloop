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
        var rval = GenerateSession.generate_default_session(app_metadata.version_string, 2)
        var drywet_audio_track = GenerateSession.generate_default_track(
            'drywet', //name
            1, //loops
            'drywet'. //id
            false, // first loop is master
            'drywet', //port name base
            2, // n dry audio channels
            2, // n wet audio channels
            0, // n direct audio channels
            false, // have dry midi
            false, // have direct midi
            false, // have send/return ports
            'text2x2x1' // drywet type
        )
        var drywet_midi_track = GenerateSession.generate_default_track(
            'drywet', //name
            1, //loops
            'drywet'. //id
            false, // first loop is master
            'drywet', //port name base
            0, // n dry audio channels
            1, // n wet audio channels
            0, // n direct audio channels
            true, // have dry midi
            false, // have direct midi
            false, // have send/return ports
            'text2x2x1' // drywet type
        )
        rval.tracks.push(drywet_audio_track)
        rval.tracks.push(drywet_midi_track)
        return rval
    }

    ShoopSessionTestCase {
        id: testcase
        name: 'DryWet'
        filename : TestFilename.test_filename()
        session: session

        function master_loop() {
            return session.tracks[0].loops[0]
        }

        // Audio loop
        function dwa_loop() {
            return session.tracks[1].loops[0]
        }

        // MIDI loop
        function dwm_loop() {
            return session.tracks[2].loops[0]
        }

        function clear() {
            master_loop().clear()
            dwa_loop().clear()
            dwm_loop().clear()
            testcase.wait_updated(session.backend)
            verify_loop_cleared(master_loop())
            verify_loop_cleared(dwa_loop())
            verify_loop_cleared(dwm_loop())
        }

        function test_cleared() {
            run_case('test_cleared', () => {
                check_backend()
                clear()
            })
        }
    }
}