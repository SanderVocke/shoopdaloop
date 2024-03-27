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
            let direct_track = GenerateSession.generate_default_track(
                "dt",
                2,
                "dt",
                false,
                "dt",
                0,
                0,
                2,
                false,
                false,
                false,
                undefined
                )
            let midi_track = GenerateSession.generate_default_track(
                "mt",
                2,
                "mt",
                false,
                "mt",
                0,
                0,
                0,
                false,
                true,
                false,
                undefined
                )
            direct_track.ports.filter(p => p.input_connectability.includes('external')).forEach(p => p.passthrough_muted = true)
            direct_track.ports.filter(p => p.output_connectability.includes('external')).forEach(p => p.muted = true)
            midi_track.ports.filter(p => p.input_connectability.includes('external')).forEach(p => p.passthrough_muted = false)
            midi_track.ports.filter(p => p.output_connectability.includes('external')).forEach(p => p.muted = false)
            let desc = GenerateSession.generate_default_session(app_metadata.version_string, null, true, 1, 1, [direct_track, midi_track])
            testcase.logger.debug(() => ("session descriptor: " + JSON.stringify(desc, null, 2)))
            return desc
        }


        ShoopSessionTestCase {
            id: testcase
            name: 'SessionDescriptor_track_controls'
            filename : TestFilename.test_filename()
            session: session

            test_fns: ({
                'test_session_descriptor_track_controls': () => {
                    check_backend()

                    testcase.wait_session_loaded(session)
                    var reference = session.initial_descriptor
                    // sample_rate not stored in the descriptor yet
                    reference['sample_rate'] = 48000

                    var actual = session.actual_session_descriptor(false, '', null)
                    reference.track_groups.forEach((g, gidx) => g.tracks.forEach((t, tidx) => t.width = actual.track_groups[gidx].tracks[tidx].width))
                    reference['track_groups'][0]['tracks'][0]['width'] = actual['track_groups'][0]['tracks'][0]['width']
                    verify(TestDeepEqual.testDeepEqual(actual, reference, session.logger.error))

                    var filename = file_io.generate_temporary_filename() + '.shl'

                    session.logger.info(() => ("Saving session to " + filename))
                    session.save_session(filename)
                    
                    testcase.wait_session_io_done()

                    session.logger.info(() => ("Re-loading session"))
                    session.load_session(filename)

                    testcase.wait_session_io_done()
                    testcase.wait_session_loaded(session)
                        
                    actual = session.actual_session_descriptor(false, '', null)

                    file_io.delete_file(filename)

                    verify(TestDeepEqual.testDeepEqual(actual, reference, session.logger.error))
                }
            })
        }
    }
}