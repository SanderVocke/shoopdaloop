import QtQuick 6.6

import './testDeepEqual.js' as TestDeepEqual
import ShoopDaLoop.Rust
import '../js/generate_session.js' as GenerateSession
import './testfilename.js' as TestFilename
import '..'

ShoopTestFile {

    QtObject {
        id: data_convert
        property list<double> data: []
    }

    TestSession {
        id: session

        anchors.fill: parent
        initial_descriptor: {
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
            return GenerateSession.generate_default_session(global_args.version_string, null, true, 1, 1, [direct_track])
        }

        ShoopSessionTestCase {
            id: testcase
            name: 'Resample'
            filename : TestFilename.test_filename()
            session: session

            function loop() { return session.sync_track.loops[0] }
            function channel() { return loop().get_audio_channels()[0] }

            function loop2() { return session.main_tracks[0].loops[0] }
            function loop2_channels() {
                if (!loop2()) return []
                var r = loop2().get_audio_channels()
                r.sort((a,b) => a.obj_id.localeCompare(b.obj_id))
                return r
            }

            function calculateSignalPowerInvariant(audioSamples) {
                const totalPower = audioSamples.reduce((sum, sample) => sum + sample ** 2, 0);
                return totalPower / audioSamples.length
            }

            test_fns: ({

                "test_resample_1chan": () => {
                    check_backend()

                    let data = Array(6000)
                    for (var i=0; i<data.length; i++) {
                        data[i] = Math.sin(i / data.length * 100)
                    }

                    var filename = ShoopRustFileIO.generate_temporary_filename() + '.wav'
                    
                    // Jump through some hoops to avoid getting a QJSValue to
                    // the file I/O. Loading via the channel forces the data type.
                    channel().load_data(data)
                    testcase.wait_updated(session.backend)
                    if (!ShoopRustFileIO.save_channels_to_soundfile(filename, 24000, [channel()])) {
                        testcase.fail("Failed to save channels to soundfile")
                    }
                    testcase.wait_updated(session.backend)
                    channel().load_data([])

                    testcase.wait_updated(session.backend)
                    testcase.wait_session_io_done()
                    testcase.wait_updated(session.backend)

                    if (!ShoopRustFileIO.load_soundfile_to_channels(filename, 48000, null,
                        [[channel()]], null, null, null)) {
                            testcase.fail("Failed to load soundfile to channels");
                        }

                    testcase.wait_updated(session.backend)
                    testcase.wait_session_io_done()
                    testcase.wait_updated(session.backend)

                    let loaded = channel().get_data()
                    verify_eq(loaded.length, 12000)

                    let ori_power = calculateSignalPowerInvariant(data)
                    let resampled_power = calculateSignalPowerInvariant(loaded)
                    verify_approx(ori_power, resampled_power, 0.1)
                },

                "test_resample_1chan_resize": () => {
                    check_backend()

                    let data = Array(6000)
                    for (var i=0; i<data.length; i++) {
                        data[i] = Math.sin(i / data.length * 100)
                    }

                    var filename = ShoopRustFileIO.generate_temporary_filename() + '.wav'

                    // Jump through some hoops to avoid getting a QJSValue to
                    // the file I/O. Loading via the channel forces the data type.
                    channel().load_data(data)
                    testcase.wait_updated(session.backend)
                    if (!ShoopRustFileIO.save_channels_to_soundfile(filename, 24000, [channel()]))  {
                        testcase.fail("Failed to save channels to soundfile")
                    }
                    testcase.wait_updated(session.backend)
                    channel().load_data([])

                    testcase.wait_updated(session.backend)
                    testcase.wait_session_io_done()
                    testcase.wait_updated(session.backend)

                    if (!ShoopRustFileIO.load_soundfile_to_channels(filename, 48000, 13000,
                        [[channel()]], null, null, null)) {
                            testcase.fail("Failed to load soundfile to channels");
                        }

                    testcase.wait_updated(session.backend)
                    testcase.wait_session_io_done()
                    testcase.wait_updated(session.backend)

                    let loaded = channel().get_data()
                    verify_eq(loaded.length, 13000)

                    let ori_power = calculateSignalPowerInvariant(data)
                    let resampled_power = calculateSignalPowerInvariant(loaded)
                    verify_approx(ori_power, resampled_power, 0.1)
                },

                "test_resample_2chan_resize": () => {
                    check_backend()
                    loop2().create_backend_loop()

                    let data = Array(6000)
                    for (var i=0; i<data.length; i++) {
                        data[i] = Math.sin(i / data.length * 100)
                    }

                    var filename = ShoopRustFileIO.generate_temporary_filename() + '.wav'
                    
                    // Jump through some hoops to avoid getting a QJSValue to
                    // the file I/O. Loading via the channel forces the data type.
                    loop2_channels()[0].load_data(data)
                    loop2_channels()[1].load_data(data)
                    testcase.wait_updated(session.backend)
                    if (!ShoopRustFileIO.save_channels_to_soundfile(filename, 24000, loop2_channels()))  {
                        testcase.fail("Failed to save channels to soundfile")
                    }
                    testcase.wait_updated(session.backend)
                    loop2_channels()[0].load_data([])
                    loop2_channels()[1].load_data([])

                    testcase.wait_updated(session.backend)
                    testcase.wait_condition(() => AppRegistries.state_registry.io_active == false)
                    testcase.wait_updated(session.backend)

                    verify_eq(loop2_channels().length, 2)
                    if (!ShoopRustFileIO.load_soundfile_to_channels(filename, 48000, 13000,
                        [[loop2_channels()[0]], [loop2_channels()[1]]], null, null, null)) {
                            testcase.fail("Failed to load soundfile to channels");
                        }

                    testcase.wait_updated(session.backend)
                    testcase.wait_condition(() => AppRegistries.state_registry.io_active == false)
                    testcase.wait_updated(session.backend)

                    let datas = loop2_channels().map(m => m.get_data())
                    verify_eq(datas[0].length, 13000)
                    verify_eq(datas[1].length, 13000)

                    let ori_power = calculateSignalPowerInvariant(data)
                    let resampled_power_0 = calculateSignalPowerInvariant(datas[0])
                    let resampled_power_1 = calculateSignalPowerInvariant(datas[1])
                    verify_approx(ori_power, resampled_power_0, 0.1)
                    verify_approx(ori_power, resampled_power_1, 0.1)
                },
            })
        }
    }
}