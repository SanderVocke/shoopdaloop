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
            let direct_track = GenerateSession.generate_default_track(
                "dt",
                1,
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
            base.tracks.push(direct_track)
            return base
        }

        ShoopSessionTestCase {
            id: testcase
            name: 'Resample'
            filename : TestFilename.test_filename()
            session: session

            function loop() { return session.tracks[0].loops[0] }
            function channel() { return loop().get_audio_channels()[0] }

            function loop2() { return session.tracks[1].loops[0] }
            function loop2_channels() {
                if (!loop2()) return []
                var r = loop2().get_audio_channels()
                r.sort((a,b) => a.obj_id.localeCompare(b.obj_id))
                return r
            }

            test_fns: ({

                "test_resample_1chan": () => {
                    check_backend()

                    let data = Array(6000)
                    for (var i=0; i<data.length; i++) {
                        data[i] = Math.sin(i / data.length * 100)
                    }

                    var filename = file_io.generate_temporary_filename() + '.wav'
                    file_io.save_data_to_soundfile(filename, 24000, [data])

                    testcase.wait_condition(() => registries.state_registry.n_saving_actions_active == 0)

                    file_io.load_soundfile_to_channels_async(filename, 48000, null,
                        [[channel()]], null, null, null)

                    testcase.wait_condition(() => registries.state_registry.n_loading_actions_active == 0)

                    let loaded = channel().get_data()
                    verify_eq(loaded.length, 12000)
                },

                "test_resample_1chan_resize": () => {
                    check_backend()

                    let data = Array(6000)
                    for (var i=0; i<data.length; i++) {
                        data[i] = Math.sin(i / data.length * 100)
                    }

                    var filename = file_io.generate_temporary_filename() + '.wav'
                    file_io.save_data_to_soundfile(filename, 24000, [data])

                    testcase.wait_condition(() => registries.state_registry.n_saving_actions_active == 0)

                    file_io.load_soundfile_to_channels_async(filename, 48000, 13000,
                        [[channel()]], null, null, null)

                    testcase.wait_condition(() => registries.state_registry.n_loading_actions_active == 0)

                    let loaded = channel().get_data()
                    verify_eq(loaded.length, 13000)
                },

                "test_resample_2chan_resize": () => {
                    check_backend()
                    loop2().create_backend_loop()

                    let data = Array(6000)
                    for (var i=0; i<data.length; i++) {
                        data[i] = Math.sin(i / data.length * 100)
                    }
                    let _data = [data, data]

                    var filename = file_io.generate_temporary_filename() + '.wav'
                    file_io.save_data_to_soundfile(filename, 24000, _data)

                    testcase.wait_condition(() => registries.state_registry.n_saving_actions_active == 0)

                    verify_eq(loop2_channels().length, 2)
                    file_io.load_soundfile_to_channels_async(filename, 48000, 13000,
                        [[loop2_channels()[0]], [loop2_channels()[1]]], null, null, null)

                    testcase.wait_condition(() => registries.state_registry.n_loading_actions_active == 0)

                    let datas = loop2_channels().map(m => m.get_data())
                    verify_eq(datas[0].length, 13000)
                    verify_eq(datas[1].length, 13000)
                },
            })
        }
    }
}