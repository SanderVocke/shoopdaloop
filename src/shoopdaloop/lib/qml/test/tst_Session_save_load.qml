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

        backend_type: ShoopConstants.AudioDriverType.Dummy
        driver_setting_overrides: {
            "sample_rate": 48000
        }

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
                    [ [{"delay": 0, "loop_id": "dt_loop_0"}], [{"delay": 1, "loop_id": "dwt_loop_0"}] ],
                    [ [{"delay": 3, "loop_id": "dwt_loop_1"}] ]
                ]
            }
            drywet_track.loops[1]['composition'] = {
                'playlists': []
            }
            let desc = GenerateSession.generate_default_session(app_metadata.version_string, null, true, 1, 1, [direct_track, drywet_track, midi_track])
            testcase.logger.debug(() => ("session descriptor: " + JSON.stringify(desc, null, 2)))
            return desc
        }

        ShoopSessionTestCase {
            id: testcase
            name: 'Session_save_load'
            filename : TestFilename.test_filename()
            session: session
            additional_when_condition: other_session && other_session.loaded

            function dt(s=session) { return s.main_tracks[0] }
            function dwt(s=session) { return s.main_tracks[1] }
            function mt(s=session) { return s.main_tracks[2] }
            function dt_loop(s=session) { return dt(s) ? dt(s).loops[0] : null }
            function dwt_loop(s=session) { return dwt(s) ? dwt(s).loops[0] : null }
            function mt_loop(s=session) { return mt(s) ? mt(s).loops[0] : null }
            function dt_loop_2(s=session) { return dt(s) ? dt(s).loops[1] : null }
            function dwt_loop_2(s=session) { return dwt(s) ? dwt(s).loops[1] : null }
            function mt_loop_2(s=session) { return mt(s) ? mt(s).loops[1] : null }

            function dt_loop_channels(s=session) {
                if (!dt_loop(s)) return []
                var r = dt_loop(s).get_audio_output_channels()
                r.sort((a,b) => a.obj_id.localeCompare(b.obj_id))
                return r
            }

            function mt_midi_channels(s=session) {
                if (!mt_loop(s)) return null
                return mt_loop(s).get_midi_channels()
            }

            function dwt_dry_loop_channels(s=session) {
                if (!dwt_loop(s)) return []
                var r = dwt_loop(s).get_audio_channels().filter(c => c.obj_id.match(/.*_dry_.*/))
                r.sort((a,b) => a.obj_id.localeCompare(b.obj_id))
                return r
            }

            function dwt_wet_loop_channels(s=session) {
                if (!dwt_loop(s)) return []
                var r = dwt_loop(s).get_audio_channels().filter(c => c.obj_id.match(/.*_wet_.*/))
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
                    testcase.wait_controlled_mode(session.backend)
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
                    mt_midi_channels()[0].load_all_midi_data(midichan)
                    testcase.wait_updated(session.backend)
                    var filename = file_io.generate_temporary_filename() + '.mid'
                    file_io.save_channel_to_midi(filename, 48000, mt_midi_channels()[0])

                    mt_loop().clear()
                    testcase.wait_updated(session.backend)
                    verify_eq(mt_midi_channels()[0].get_recorded_midi_msgs(), [])

                    file_io.load_midi_to_channels(filename, 48000, [mt_midi_channels()[0]], null, null, null)
                    testcase.wait_updated(session.backend)

                    // Storing in MIDI files is not sample-accurate but should be quite close
                    let data = mt_midi_channels()[0].get_recorded_midi_msgs()
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
                    mt_midi_channels()[0].load_all_midi_data(midichan)
                    testcase.wait_updated(session.backend)
                    var filename = file_io.generate_temporary_filename() + '.smf'
                    file_io.save_channel_to_midi(filename, 48000, mt_midi_channels()[0])

                    mt_loop().clear()
                    testcase.wait_updated(session.backend)
                    verify_eq(mt_midi_channels()[0].get_recorded_midi_msgs(), [])

                    file_io.load_midi_to_channels(filename, 48000, [mt_midi_channels()[0]], null, null, null)
                    testcase.wait_updated(session.backend)

                    // Storing in MIDI files is not sample-accurate but should be quite close
                    let data = mt_midi_channels()[0].get_recorded_midi_msgs()
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
                    mt_midi_channels()[0].load_all_midi_data(midichan)
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
                    
                    verify_eq(dt_loop_channels()[0].get_data_list(), [])
                    verify_eq(dt_loop_channels()[1].get_data_list(), [])
                    verify_eq(mt_midi_channels()[0].get_recorded_midi_msgs(), [])
                    verify_eq(dwt_dry_loop_channels()[0].get_data_list(), [])
                    verify_eq(dwt_dry_loop_channels()[1].get_data_list(), [])
                    verify_eq(dwt_wet_loop_channels()[0].get_data_list(), [])
                    verify_eq(dwt_wet_loop_channels()[1].get_data_list(), [])

                    session.load_session(filename)
                    testcase.wait_session_loaded(session)
                    testcase.wait_updated(session.backend)

                    verify_true(dt_loop_2().maybe_composite_loop)
                    verify_true(dwt_loop_2().maybe_composite_loop)
                    verify_eq(dt_loop_2().maybe_composite_loop.all_loops, new Set([dt_loop(), dwt_loop()]))
                    verify_eq(dwt_loop_2().maybe_composite_loop.all_loops, new Set())
                    verify_approx(dt_loop_channels()[0].get_data_list(), [0.1, 0.2, 0.3, 0.4])
                    verify_approx(dt_loop_channels()[1].get_data_list(), [0.4, 0.3, 0.2, 0.1])
                    verify_approx(dwt_dry_loop_channels()[0].get_data_list(), [0.5, 0.6, 0.7, 0.8])
                    verify_approx(dwt_dry_loop_channels()[1].get_data_list(), [0.8, 0.7, 0.6, 0.5])
                    verify_approx(dwt_wet_loop_channels()[0].get_data_list(), [0.9, 0.10, 0.11, 0.12])
                    verify_approx(dwt_wet_loop_channels()[1].get_data_list(), [0.12, 0.11, 0.10, 0.9])
                    verify_eq(mt_midi_channels()[0].get_recorded_midi_msgs(), midichan)
                },

                "test_save_load_session_audio_and_midi_resampled": () => {
                    check_backend()
                    verify(other_session.backend && other_session.backend.initialized, "resampled backend not initialized")

                    let midichan = [
                        { 'time': 120, 'data': [0x90, 70,  70]  },
                        { 'time': 180, 'data': [0x80, 60,  60]  }
                    ]
                    let midichan_resampled = [
                        { 'time': 80, 'data': [0x90, 70,  70]  },
                        { 'time': 120, 'data': [0x80, 60,  60]  }
                    ]

                    mt_midi_channels()[0].load_all_midi_data(midichan)
                    let _data = [0.0, 0.0, 0.0, 0.0,
                            0.0, 0.0, 0.0, 0.0,
                            0.0, 0.0, 0.0, 0.0] // 12 samples
                    dt_loop_channels()[0].load_data(_data) // 12 samples
                    dt_loop_channels()[1].load_data(_data)
                    dwt_dry_loop_channels()[0].load_data(_data)
                    dwt_dry_loop_channels()[1].load_data(_data)
                    dwt_wet_loop_channels()[0].load_data(_data)
                    dwt_wet_loop_channels()[1].load_data(_data)
                    dt_loop().set_length(6)
                    dt_loop_channels()[0].set_n_preplay_samples(4)
                    dt_loop_channels()[0].set_start_offset(4)
                    dt_loop_channels()[1].set_n_preplay_samples(12)
                    dt_loop_channels()[1].set_start_offset(12)
                    dwt_loop().set_length(12)
                    testcase.wait_updated(session.backend)

                    var filename = file_io.generate_temporary_filename() + '.shl'
                    session.save_session(filename)

                    testcase.wait_session_io_done()
                    dt_loop().clear()
                    dwt_loop().clear()
                    mt_loop().clear()
                    testcase.wait_updated(session.backend)

                    // Load into the other session, which has 3/4 the sample rate of this one
                    other_session.load_session(filename, true)
                    testcase.wait_session_loaded(other_session)
                    testcase.wait_updated(other_session.backend)

                    verify_true(dt_loop_2(other_session).maybe_composite_loop)
                    verify_true(dwt_loop_2(other_session).maybe_composite_loop)
                    verify_eq(dt_loop_2(other_session).maybe_composite_loop.all_loops, new Set([dt_loop(), dwt_loop()]))
                    verify_eq(dwt_loop_2(other_session).maybe_composite_loop.all_loops, new Set())
                    verify_eq(dt_loop_channels(other_session)[0].data_length, 8)
                    verify_eq(dt_loop_channels(other_session)[1].data_length, 8)
                    verify_eq(dt_loop_channels(other_session)[0].n_preplay_samples, 3)
                    verify_eq(dt_loop_channels(other_session)[1].n_preplay_samples, 8)
                    verify_eq(dt_loop_channels(other_session)[0].start_offset, 3)
                    verify_eq(dt_loop_channels(other_session)[1].start_offset, 8)
                    verify_eq(dwt_dry_loop_channels(other_session)[0].data_length, 8)
                    verify_eq(dwt_dry_loop_channels(other_session)[1].data_length, 8)
                    verify_eq(dwt_wet_loop_channels(other_session)[0].data_length, 8)
                    verify_eq(dwt_wet_loop_channels(other_session)[1].data_length, 8)
                    verify_eq(dt_loop(other_session).length, 4)
                    verify_eq(dwt_loop(other_session).length, 8)
                    verify_eq(mt_midi_channels(other_session)[0].get_recorded_midi_msgs(), midichan_resampled)
                },

                "test_save_load_track_controls": () => {
                    check_backend()

                    mt().control_widget.mute = true
                    mt().control_widget.monitor = true
                    dt().control_widget.mute = true
                    dt().control_widget.monitor = true
                    dt().control_widget.set_gain(0.5)
                    dt().control_widget.set_balance(0.9)
                    dwt().control_widget.monitor = true
                    dwt().control_widget.mute = true
                    dwt().control_widget.set_gain(0.4)
                    dwt().control_widget.set_balance(0.8)
                    testcase.wait_updated(session.backend)

                    verify_approx(dt().control_widget.last_pushed_gain, 0.5)
                    verify_approx(dt().control_widget.last_pushed_out_stereo_balance, 0.9)
                    verify_approx(dwt().control_widget.last_pushed_gain, 0.4)
                    verify_approx(dwt().control_widget.last_pushed_out_stereo_balance, 0.8)

                    var filename = file_io.generate_temporary_filename() + '.shl'
                    session.save_session(filename)

                    testcase.wait_session_io_done()
                    mt().control_widget.mute = false
                    mt().control_widget.monitor = false
                    dt().control_widget.mute = false
                    dt().control_widget.monitor = false
                    dt().control_widget.set_gain(1.0)
                    dt().control_widget.set_balance(0.0)
                    dwt().control_widget.monitor = false
                    dwt().control_widget.mute = false
                    dwt().control_widget.set_gain(1.0)
                    dwt().control_widget.set_balance(0.0)
                    testcase.wait_updated(session.backend)

                    verify_approx(dt().control_widget.last_pushed_gain, 1.0)
                    verify_approx(dt().control_widget.last_pushed_out_stereo_balance, 0.0)
                    verify_approx(dwt().control_widget.last_pushed_gain, 1.0)
                    verify_approx(dwt().control_widget.last_pushed_out_stereo_balance, 0.0)

                    session.load_session(filename)
                    testcase.wait_session_loaded(session)
                    testcase.wait_updated(session.backend)

                    verify_eq(mt().control_widget.mute, true)
                    verify_eq(mt().control_widget.monitor, true)
                    verify_eq(dt().control_widget.mute, true)
                    verify_eq(dt().control_widget.monitor, true)
                    verify_approx(dt().control_widget.last_pushed_gain, 0.5)
                    verify_approx(dt().control_widget.last_pushed_out_stereo_balance, 0.9)
                    verify_eq(dwt().control_widget.monitor, true)
                    verify_eq(dwt().control_widget.mute, true)
                    verify_approx(dwt().control_widget.last_pushed_gain, 0.4)
                    verify_approx(dwt().control_widget.last_pushed_out_stereo_balance, 0.8)
                },
            })
        }
    }

    Session {
        id: other_session
        backend_type: ShoopConstants.AudioDriverType.Dummy
        driver_setting_overrides: {
            "sample_rate": 32000
        }
        initial_descriptor: GenerateSession.generate_default_session(app_metadata.version_string, null, true, 1, 1, [])
    }
}