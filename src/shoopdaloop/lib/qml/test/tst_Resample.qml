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
            return base
        }

        ShoopSessionTestCase {
            id: testcase
            name: 'Resample'
            filename : TestFilename.test_filename()
            session: session

            function loop() { return session.tracks[0].loops[0] }
            function channel() { return loop().get_audio_channels()[0] }

            function test_resample_1chan() {
                run_case("test_resample_1chan" , () => {
                    check_backend()

                    let data = Array(6000)
                    for (var i=0; i<data.length; i++) {
                        data[i] = Math.sin(i / data.length * 100)
                    }

                    var filename = file_io.generate_temporary_filename() + '.wav'
                    file_io.save_data_to_soundfile(filename, 24000, [data])

                    testcase.wait_condition(() => registries.state_registry.n_saving_actions_active == 0)

                    file_io.load_soundfile_to_channels_async(filename, 48000, 12000,
                        [[channel()]], null, null, null)

                    testcase.wait_condition(() => registries.state_registry.n_loading_actions_active == 0)

                    let loaded = channel().get_data()
                    verify_eq(loaded.length == 12000)
                    let expect = Array(12000)
                    for (var i=0; i<expect.length; i++) {
                        data[i] = Math.sin(i / data.length * 100)
                    }
                    verify_approx(loaded, expect)
                })
            }
        }
    }
}