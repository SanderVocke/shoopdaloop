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
            "tut",
            1,
            "tut",
            false,
            "tut",
            2,
            2,
            0,
            false,
            false,
            false,
            "test2x2x1"
            )
        base.tracks.push(track)
        testcase.logger.debug("session descriptor: " + JSON.stringify(base, null, 2))
        return base
    }

    ShoopSessionTestCase {
        id: testcase
        name: 'TrackControlAndLoop_drywet'
        filename : TestFilename.test_filename()
        session: session

        property var tut : session.tracks[1]
        property var lut : session.tracks[1].loops[0] // LoopWidget

        function wet_channels() {
            if (!lut) return []
            var r = lut.get_audio_channels().filter(c => c.obj_id.match(/.*_wet_.*/))
            r.sort((a,b) => a.obj_id.localeCompare(b.obj_id))
            return r
        }

        function dry_channels() {
            if (!lut) return []
            var r = lut.get_audio_channels().filter(c => c.obj_id.match(/.*_dry_.*/))
            r.sort((a,b) => a.obj_id.localeCompare(b.obj_id))
            return r
        }
        
        function tut_control() {
            return session.get_track_control_widget(1)
        }

        RegistryLookup {
            id: lookup_input_port_1
            registry: session.objects_registry
            key: "tut_audio_dry_in_1"
        }
        property alias input_port_1: lookup_input_port_1.object

        RegistryLookup {
            id: lookup_input_port_2
            registry: session.objects_registry
            key: "tut_audio_dry_in_2"
        }
        property alias input_port_2: lookup_input_port_2.object

        RegistryLookup {
            id: lookup_output_port_1
            registry: session.objects_registry
            key: "tut_audio_wet_out_1"
        }
        property alias output_port_1: lookup_output_port_1.object

        RegistryLookup {
            id: lookup_output_port_2
            registry: session.objects_registry
            key: "tut_audio_wet_out_2"
        }
        property alias output_port_2: lookup_output_port_2.object

        RegistryLookup {
            id: lookup_fx
            registry: session.objects_registry
            key: "tut_fx_chain"
        }
        property alias fx: lookup_fx.object

        function initTestCase() {
            session.backend.dummy_enter_controlled_mode()
            verify_throw(input_port_1)
            verify_throw(input_port_2)
            verify_throw(output_port_1)
            verify_throw(output_port_2)
            verify_throw(fx)
            verify_throw(lut)
            verify_throw(tut)
            reset()
        }

        function reset_track(track_idx) {
            let t = session.tracks[track_idx]
            let c = session.get_track_control_widget(track_idx)
            c.input_balance = 0.0
            c.output_balance = 0.0
            c.volume_dB = 0.0
            c.input_volume_dB = 0.0
            c.monitor = false
            c.mute = false
        }

        function reset_loop(loopwidget) {
            loopwidget.transition(Types.LoopMode.Stopped, 0, false)
            loopwidget.clear(0)
            session.backend.dummy_wait_process()
        }

        function reset() {
            reset_track(0)
            reset_track(1)
            reset_loop(lut)
            session.backend.dummy_wait_process()
        }

        function test_drywet_record_no_monitor() {
            run_case('test_drywet_record_no_monitor', () => {
                check_backend()
                reset()
                tut_control().monitor = false
                tut_control().mute = false
                lut.transition(Types.LoopMode.Recording, 0, false)
                testcase.wait(50)

                input_port_1.dummy_queue_data([1, 2, 3, 4])
                input_port_2.dummy_queue_data([4, 3, 2, 1])
                output_port_1.dummy_request_data(4)
                output_port_2.dummy_request_data(4)
                session.backend.dummy_request_controlled_frames(4)
                session.backend.dummy_wait_process()

                let out1 = output_port_1.dummy_dequeue_data(4)
                let out2 = output_port_2.dummy_dequeue_data(4)
                let dry1 = dry_channels()[0].get_data()
                let dry2 = dry_channels()[1].get_data()
                let wet1 = wet_channels()[0].get_data()
                let wet2 = wet_channels()[1].get_data()
                verify_throw(fx.active)

                verify_eq(out1, [0, 0, 0, 0])
                verify_eq(out2, [0, 0, 0, 0])
                verify_eq(dry1, [1, 2, 3, 4])
                verify_eq(dry2, [4, 3, 2, 1])
                verify_eq(wet1, [1, 2, 3, 4])
                verify_eq(wet2, [4, 3, 2, 1])

            })
        }

        function test_drywet_record_monitor() {
            run_case('test_drywet_record_monitor', () => {
                check_backend()
                reset()
                tut_control().monitor = true
                tut_control().mute = false
                lut.transition(Types.LoopMode.Recording, 0, false)
                testcase.wait(50)

                input_port_1.dummy_queue_data([1, 2, 3, 4])
                input_port_2.dummy_queue_data([4, 3, 2, 1])
                output_port_1.dummy_request_data(4)
                output_port_2.dummy_request_data(4)
                session.backend.dummy_request_controlled_frames(4)
                session.backend.dummy_wait_process()

                let out1 = output_port_1.dummy_dequeue_data(4)
                let out2 = output_port_2.dummy_dequeue_data(4)
                let dry1 = dry_channels()[0].get_data()
                let dry2 = dry_channels()[1].get_data()
                let wet1 = wet_channels()[0].get_data()
                let wet2 = wet_channels()[1].get_data()
                verify_throw(fx.active)

                verify_eq(out1, [1, 2, 3, 4])
                verify_eq(out2, [4, 3, 2, 1])
                verify_eq(dry1, [1, 2, 3, 4])
                verify_eq(dry2, [4, 3, 2, 1])
                verify_eq(wet1, [1, 2, 3, 4])
                verify_eq(wet2, [4, 3, 2, 1])

            })
        }

        function test_drywet_play_no_monitor() {
            run_case('test_drywet_play_no_monitor', () => {
                check_backend()
                reset()
                tut_control().monitor = false
                tut_control().mute = false
                dry_channels()[0].load_data([50, 60, 70, 80])
                dry_channels()[1].load_data([80, 70, 60, 50])
                wet_channels()[0].load_data([5, 6, 7, 8])
                wet_channels()[1].load_data([8, 7, 6, 5])
                lut.set_length(4)
                lut.transition(Types.LoopMode.Playing, 0, false)
                testcase.wait(50)

                input_port_1.dummy_queue_data([1, 2, 3, 4])
                input_port_2.dummy_queue_data([4, 3, 2, 1])
                output_port_1.dummy_request_data(4)
                output_port_2.dummy_request_data(4)
                session.backend.dummy_request_controlled_frames(4)
                session.backend.dummy_wait_process()

                let out1 = output_port_1.dummy_dequeue_data(4)
                let out2 = output_port_2.dummy_dequeue_data(4)
                let dry1 = dry_channels()[0].get_data()
                let dry2 = dry_channels()[1].get_data()
                let wet1 = wet_channels()[0].get_data()
                let wet2 = wet_channels()[1].get_data()
                verify_throw(!fx.active)

                verify_eq(out1, [5, 6, 7, 8])
                verify_eq(out2, [8, 7, 6, 5])
                verify_eq(dry1, [50, 60, 70, 80])
                verify_eq(dry2, [80, 70, 60, 50])
                verify_eq(wet1, [5, 6, 7, 8])
                verify_eq(wet2, [8, 7, 6, 5])

            })
        }

        function test_drywet_play_monitor() {
            run_case('test_drywet_play_monitor', () => {
                check_backend()
                reset()
                tut_control().monitor = true
                tut_control().mute = false
                dry_channels()[0].load_data([50, 60, 70, 80])
                dry_channels()[1].load_data([80, 70, 60, 50])
                wet_channels()[0].load_data([5, 6, 7, 8])
                wet_channels()[1].load_data([8, 7, 6, 5])
                lut.set_length(4)
                lut.transition(Types.LoopMode.Playing, 0, false)
                testcase.wait(50)

                input_port_1.dummy_queue_data([1, 2, 3, 4])
                input_port_2.dummy_queue_data([4, 3, 2, 1])
                output_port_1.dummy_request_data(4)
                output_port_2.dummy_request_data(4)
                session.backend.dummy_request_controlled_frames(4)
                session.backend.dummy_wait_process()

                let out1 = output_port_1.dummy_dequeue_data(4)
                let out2 = output_port_2.dummy_dequeue_data(4)
                let dry1 = dry_channels()[0].get_data()
                let dry2 = dry_channels()[1].get_data()
                let wet1 = wet_channels()[0].get_data()
                let wet2 = wet_channels()[1].get_data()

                verify_eq(out1, [6, 8, 10, 12])
                verify_eq(out2, [12, 10, 8, 6])
                verify_eq(dry1, [50, 60, 70, 80])
                verify_eq(dry2, [80, 70, 60, 50])
                verify_eq(wet1, [5, 6, 7, 8])
                verify_eq(wet2, [8, 7, 6, 5])
                verify_throw(fx.active)

            })
        }

        function test_drywet_playdry_no_monitor() {
            run_case('test_drywet_playdry_no_monitor', () => {
                check_backend()
                reset()
                tut_control().monitor = false
                tut_control().mute = false
                dry_channels()[0].load_data([50, 60, 70, 80])
                dry_channels()[1].load_data([80, 70, 60, 50])
                wet_channels()[0].load_data([5, 6, 7, 8])
                wet_channels()[1].load_data([8, 7, 6, 5])
                lut.set_length(4)
                lut.transition(Types.LoopMode.PlayingDryThroughWet, 0, false)
                testcase.wait(50)

                input_port_1.dummy_queue_data([1, 2, 3, 4])
                input_port_2.dummy_queue_data([4, 3, 2, 1])
                output_port_1.dummy_request_data(4)
                output_port_2.dummy_request_data(4)
                session.backend.dummy_request_controlled_frames(4)
                session.backend.dummy_wait_process()

                let l = session.get_track_control_widget(1).control_logic

                let out1 = output_port_1.dummy_dequeue_data(4)
                let out2 = output_port_2.dummy_dequeue_data(4)
                let dry1 = dry_channels()[0].get_data()
                let dry2 = dry_channels()[1].get_data()
                let wet1 = wet_channels()[0].get_data()
                let wet2 = wet_channels()[1].get_data()

                verify_throw(fx.active)
                verify_eq(out1, [50, 60, 70, 80])
                verify_eq(out2, [80, 70, 60, 50])
                verify_eq(dry1, [50, 60, 70, 80])
                verify_eq(dry2, [80, 70, 60, 50])
                verify_eq(wet1, [5, 6, 7, 8])
                verify_eq(wet2, [8, 7, 6, 5])

            })
        }

        function test_drywet_playdry_monitor() {
            run_case('test_drywet_playdry_monitor', () => {
                check_backend()
                reset()
                tut_control().monitor = true
                tut_control().mute = false
                dry_channels()[0].load_data([50, 60, 70, 80])
                dry_channels()[1].load_data([80, 70, 60, 50])
                wet_channels()[0].load_data([5, 6, 7, 8])
                wet_channels()[1].load_data([8, 7, 6, 5])
                lut.set_length(4)
                lut.transition(Types.LoopMode.PlayingDryThroughWet, 0, false)
                testcase.wait(50)

                input_port_1.dummy_queue_data([1, 2, 3, 4])
                input_port_2.dummy_queue_data([4, 3, 2, 1])
                output_port_1.dummy_request_data(4)
                output_port_2.dummy_request_data(4)
                session.backend.dummy_request_controlled_frames(4)
                session.backend.dummy_wait_process()

                let out1 = output_port_1.dummy_dequeue_data(4)
                let out2 = output_port_2.dummy_dequeue_data(4)
                let dry1 = dry_channels()[0].get_data()
                let dry2 = dry_channels()[1].get_data()
                let wet1 = wet_channels()[0].get_data()
                let wet2 = wet_channels()[1].get_data()

                verify_throw(fx.active)
                verify_eq(out1, [51, 62, 73, 84])
                verify_eq(out2, [84, 73, 62, 51])
                verify_eq(dry1, [50, 60, 70, 80])
                verify_eq(dry2, [80, 70, 60, 50])
                verify_eq(wet1, [5, 6, 7, 8])
                verify_eq(wet2, [8, 7, 6, 5])

            })
        }

        function test_drywet_rerecord_no_monitor() {
            run_case('test_drywet_rerecord_no_monitor', () => {
                check_backend()
                reset()
                tut_control().monitor = false
                tut_control().mute = false
                dry_channels()[0].load_data([50, 60, 70, 80])
                dry_channels()[1].load_data([80, 70, 60, 50])
                wet_channels()[0].load_data([5, 6, 7, 8])
                wet_channels()[1].load_data([8, 7, 6, 5])
                lut.set_length(4)
                lut.transition(Types.LoopMode.RecordingDryIntoWet, 0, false)
                testcase.wait(50)

                input_port_1.dummy_queue_data([1, 2, 3, 4])
                input_port_2.dummy_queue_data([4, 3, 2, 1])
                output_port_1.dummy_request_data(4)
                output_port_2.dummy_request_data(4)
                session.backend.dummy_request_controlled_frames(4)
                session.backend.dummy_wait_process()

                let l = session.get_track_control_widget(1).control_logic

                let out1 = output_port_1.dummy_dequeue_data(4)
                let out2 = output_port_2.dummy_dequeue_data(4)
                let dry1 = dry_channels()[0].get_data()
                let dry2 = dry_channels()[1].get_data()
                let wet1 = wet_channels()[0].get_data()
                let wet2 = wet_channels()[1].get_data()

                verify_throw(fx.active)
                verify_eq(out1, [50, 60, 70, 80])
                verify_eq(out2, [80, 70, 60, 50])
                verify_eq(dry1, [50, 60, 70, 80])
                verify_eq(dry2, [80, 70, 60, 50])
                verify_eq(wet1, [50, 60, 70, 80])
                verify_eq(wet2, [80, 70, 60, 50])

            })
        }

        function test_drywet_rerecord_monitor() {
            run_case('test_drywet_rerecord_monitor', () => {
                check_backend()
                reset()
                tut_control().monitor = true
                tut_control().mute = false
                dry_channels()[0].load_data([50, 60, 70, 80])
                dry_channels()[1].load_data([80, 70, 60, 50])
                wet_channels()[0].load_data([5, 6, 7, 8])
                wet_channels()[1].load_data([8, 7, 6, 5])
                lut.set_length(4)
                lut.transition(Types.LoopMode.RecordingDryIntoWet, 0, false)
                testcase.wait(50)

                input_port_1.dummy_queue_data([1, 2, 3, 4])
                input_port_2.dummy_queue_data([4, 3, 2, 1])
                output_port_1.dummy_request_data(4)
                output_port_2.dummy_request_data(4)
                session.backend.dummy_request_controlled_frames(4)
                session.backend.dummy_wait_process()

                let out1 = output_port_1.dummy_dequeue_data(4)
                let out2 = output_port_2.dummy_dequeue_data(4)
                let dry1 = dry_channels()[0].get_data()
                let dry2 = dry_channels()[1].get_data()
                let wet1 = wet_channels()[0].get_data()
                let wet2 = wet_channels()[1].get_data()

                verify_throw(fx.active)
                verify_eq(out1, [50, 60, 70, 80])
                verify_eq(out2, [80, 70, 60, 50])
                verify_eq(dry1, [50, 60, 70, 80])
                verify_eq(dry2, [80, 70, 60, 50])
                verify_eq(wet1, [50, 60, 70, 80])
                verify_eq(wet2, [80, 70, 60, 50])

            })
        }
    }
}