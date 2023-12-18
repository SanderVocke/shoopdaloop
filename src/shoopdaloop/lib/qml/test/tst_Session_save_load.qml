import QtQuick 6.3
import QtTest 1.0
import ShoopDaLoop.PythonBackend

import './testDeepEqual.js' as TestDeepEqual
import ShoopConstants
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
            let drywet_track = GenerateSession.generate_default_track(
                "dwt",
                2,
                "dwt",
                false,
                "dwt",
                2,
                2,
                0,
                false,
                false,
                false,
                "test2x2x1"
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
            direct_track.loops[1]['composition'] = {
                'playlists': [
                    [ {"delay": 0, "loop_id": "dt_loop_0"}, {"delay": 1, "loop_id": "dwt_loop_0"} ],
                    [ {"delay": 3, "loop_id": "dwt_loop_1"} ]
                ]
            }
            drywet_track.loops[1]['composition'] = {
                'playlists': []
            }
            base.tracks.push(direct_track)
            base.tracks.push(drywet_track)
            base.tracks.push(midi_track)
            testcase.logger.debug(() => ("session descriptor: " + JSON.stringify(base, null, 2)))
            return base
        }

        ShoopSessionTestCase {
            id: testcase
            name: 'Session_save_load'
            filename : TestFilename.test_filename()
            session: session

            function dt() { return session.tracks[1] }
            function dwt() { return session.tracks[2] }
            function mt() { return session.tracks[3] }
            function dt_loop() { return dt() ? dt().loops[0] : null }
            function dwt_loop() { return dwt() ? dwt().loops[0] : null }
            function mt_loop() { return mt() ? mt().loops[0] : null }
            function dt_loop_2() { return dt() ? dt().loops[1] : null }
            function dwt_loop_2() { return dwt() ? dwt().loops[1] : null }
            function mt_loop_2() { return mt() ? mt().loops[1] : null }

            function dt_loop_channels() {
                if (!dt_loop()) return []
                var r = dt_loop().get_audio_output_channels()
                r.sort((a,b) => a.obj_id.localeCompare(b.obj_id))
                return r
            }

            function mt_midi_channels() {
                if (!mt_loop()) return null
                return mt_loop().get_midi_channels()
            }

            function dwt_dry_loop_channels() {
                if (!dwt_loop()) return []
                var r = dwt_loop().get_audio_channels().filter(c => c.obj_id.match(/.*_dry_.*/))
                r.sort((a,b) => a.obj_id.localeCompare(b.obj_id))
                return r
            }

            function dwt_wet_loop_channels() {
                if (!dwt_loop()) return []
                var r = dwt_loop().get_audio_channels().filter(c => c.obj_id.match(/.*_wet_.*/))
                r.sort((a,b) => a.obj_id.localeCompare(b.obj_id))
                return r
            }

            RegistryLookup {
                id: lookup_input_port_1
                registry: registries.objects_registry
                key: "dt_direct_in_1"
            }
            property alias input_port_1: lookup_input_port_1.object

            testcase_init_fn: () => {
                run_case("initTestCase" , () => {
                    session.backend.dummy_enter_controlled_mode()
                    verify_true(dt())
                    verify_true(dwt())
                    verify_true(mt())
                    verify_true(dt_loop())
                    verify_true(dwt_loop())
                    verify_true(mt_loop())
                    verify_true(mt_loop_2())
                    dt_loop().create_backend_loop()
                    dwt_loop().create_backend_loop()
                    mt_loop().create_backend_loop()
                    verify_eq(dt_loop_channels().length, 2)
                    verify_eq(dwt_dry_loop_channels().length, 2)
                    verify_eq(dwt_wet_loop_channels().length, 2)
                    verify_eq(mt_midi_channels().length, 1)
                    verify_true(dt_loop_2().maybe_composite_loop)
                    verify_true(dwt_loop_2().maybe_composite_loop)
                    verify_eq(dt_loop_2().maybe_composite_loop.all_loops, new Set([dt_loop(), dwt_loop()]))
                    verify_eq(dwt_loop_2().maybe_composite_loop.all_loops, new Set())
                })
            }

            test_fns: ({

                "test_save_load_non_sample_accurate_midi": () => {
                    check_backend()

                    let midichan = [
                        { 'time': 101, 'data': [0x90, 70,  70]  },
                        { 'time': 201, 'data': [0x80, 60,  60]  }
                    ]
                    mt_midi_channels()[0].load_data(midichan)
                    testcase.wait_updated(session.backend)
                    var filename = file_io.generate_temporary_filename() + '.mid'
                    file_io.save_channel_to_midi(filename, 48000, mt_midi_channels()[0])

                    mt_loop().clear()
                    testcase.wait_updated(session.backend)
                    verify_eq(mt_midi_channels()[0].get_data(), [])

                    file_io.load_midi_to_channel(filename, 48000, mt_midi_channels()[0], null, null)
                    testcase.wait_updated(session.backend)

                    // Storing in MIDI files is not sample-accurate but should be quite close
                    let data = mt_midi_channels()[0].get_data()
                    verify_eq(data.length, 2)
                    verify_true(data[0].time >= 100 && data[0].time <= 102)
                    verify_true(data[1].time >= 200 && data[0].time <= 202)
                    verify_eq(data[0].data, [0x90, 70,  70])
                    verify_eq(data[1].data, [0x80, 60,  60])
                },

                "test_save_load_sample_accurate_midi": () => {
                    check_backend()

                    let midichan = [
                        { 'time': 101, 'data': [0x90, 70,  70]  },
                        { 'time': 201, 'data': [0x80, 60,  60]  }
                    ]
                    mt_midi_channels()[0].load_data(midichan)
                    testcase.wait_updated(session.backend)
                    var filename = file_io.generate_temporary_filename() + '.smf'
                    file_io.save_channel_to_midi(filename, 48000, mt_midi_channels()[0])

                    mt_loop().clear()
                    testcase.wait_updated(session.backend)
                    verify_eq(mt_midi_channels()[0].get_data(), [])

                    file_io.load_midi_to_channel(filename, 48000, mt_midi_channels()[0], null, null)
                    testcase.wait_updated(session.backend)

                    // Storing in MIDI files is not sample-accurate but should be quite close
                    let data = mt_midi_channels()[0].get_data()
                    verify_eq(data.length, 2)
                    verify_true(data[0].time >= 100 && data[0].time <= 102)
                    verify_true(data[1].time >= 200 && data[0].time <= 202)
                    verify_eq(data[0].data, [0x90, 70,  70])
                    verify_eq(data[1].data, [0x80, 60,  60])
                },

                "test_save_load_session_audio_and_midi": () => {
                    check_backend()

                    let midichan = [
                        { 'time': 101, 'data': [0x90, 70,  70]  },
                        { 'time': 201, 'data': [0x80, 60,  60]  }
                    ]
                    mt_midi_channels()[0].load_data(midichan)
                    dt_loop_channels()[0].load_data([0.1, 0.2, 0.3, 0.4])
                    dt_loop_channels()[1].load_data([0.4, 0.3, 0.2, 0.1])
                    dwt_dry_loop_channels()[0].load_data([0.5, 0.6, 0.7, 0.8])
                    dwt_dry_loop_channels()[1].load_data([0.8, 0.7, 0.6, 0.5])
                    dwt_wet_loop_channels()[0].load_data([0.9, 0.10, 0.11, 0.12])
                    dwt_wet_loop_channels()[1].load_data([0.12, 0.11, 0.10, 0.9])
                    dt_loop().set_length(2)
                    dt_loop_channels()[0].set_n_preplay_samples(1)
                    dt_loop_channels()[0].set_start_offset(2)
                    dt_loop_channels()[1].set_n_preplay_samples(1)
                    dt_loop_channels()[1].set_start_offset(2)
                    dwt_loop().set_length(4)
                    testcase.wait_updated(session.backend)

                    var filename = file_io.generate_temporary_filename() + '.shl'
                    session.save_session(filename)

                    testcase.wait_session_io_done()
                    dt_loop().clear()
                    dwt_loop().clear()
                    mt_loop().clear()
                    testcase.wait_updated(session.backend)
                    
                    verify_eq(dt_loop_channels()[0].get_data(), [])
                    verify_eq(dt_loop_channels()[1].get_data(), [])
                    verify_eq(mt_midi_channels()[0].get_data(), [])
                    verify_eq(dwt_dry_loop_channels()[0].get_data(), [])
                    verify_eq(dwt_dry_loop_channels()[1].get_data(), [])
                    verify_eq(dwt_wet_loop_channels()[0].get_data(), [])
                    verify_eq(dwt_wet_loop_channels()[1].get_data(), [])

                    session.load_session(filename)
                    testcase.wait_session_loaded(session)
                    testcase.wait_updated(session.backend)

                    verify_true(dt_loop_2().maybe_composite_loop)
                    verify_true(dwt_loop_2().maybe_composite_loop)
                    verify_eq(dt_loop_2().maybe_composite_loop.all_loops, new Set([dt_loop(), dwt_loop()]))
                    verify_eq(dwt_loop_2().maybe_composite_loop.all_loops, new Set())
                    verify_approx(dt_loop_channels()[0].get_data(), [0.1, 0.2, 0.3, 0.4])
                    verify_approx(dt_loop_channels()[1].get_data(), [0.4, 0.3, 0.2, 0.1])
                    verify_approx(dwt_dry_loop_channels()[0].get_data(), [0.5, 0.6, 0.7, 0.8])
                    verify_approx(dwt_dry_loop_channels()[1].get_data(), [0.8, 0.7, 0.6, 0.5])
                    verify_approx(dwt_wet_loop_channels()[0].get_data(), [0.9, 0.10, 0.11, 0.12])
                    verify_approx(dwt_wet_loop_channels()[1].get_data(), [0.12, 0.11, 0.10, 0.9])
                    verify_eq(mt_midi_channels()[0].get_data(), midichan)
                },
            })
        }
    }
}