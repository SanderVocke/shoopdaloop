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
            2, // dry audio
            2, // wet audio
            0,
            true, // dry MIDI
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
        name: 'TrackControl_drywet'
        filename : TestFilename.test_filename()
        session: session

        property var tut : session.tracks[1]
        
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
            id: lookup_midi_input_port
            registry: session.objects_registry
            key: "tut_direct_dry_in"
        }
        property alias midi_input_port: lookup_midi_input_port.object

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
            verify_throw(midi_input_port)
            verify_throw(fx)
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

        function reset() {
            reset_track(0)
            reset_track(1)
            session.backend.dummy_wait_process()
        }

        function test_drywet_audio_monitor() {
            run_case('test_drywet_audio_monitor', () => {
                check_backend()
                reset()
                tut_control().monitor = true
                tut_control().mute = false
                testcase.wait(50)

                input_port_1.dummy_queue_data([1, 2, 3, 4])
                input_port_2.dummy_queue_data([4, 3, 2, 1])
                output_port_1.dummy_request_data(4)
                output_port_2.dummy_request_data(4)
                session.backend.dummy_request_controlled_frames(4)
                session.backend.dummy_wait_process()

                let out1 = output_port_1.dummy_dequeue_data(4)
                let out2 = output_port_2.dummy_dequeue_data(4)

                verify_eq(out1, [1, 2, 3, 4])
                verify_eq(out2, [4, 3, 2, 1])
                verify_throw(fx.active)

            })
        }

        function test_drywet_audio_monitor_input_volume() {
            run_case('test_drywet_audio_monitor_input_volume', () => {
                check_backend()
                reset()
                tut_control().monitor = true
                tut_control().mute = false
                tut_control().input_volume_dB = 6.0
                testcase.wait(50)

                input_port_1.dummy_queue_data([1, 2, 3, 4])
                input_port_2.dummy_queue_data([4, 3, 2, 1])
                output_port_1.dummy_request_data(4)
                output_port_2.dummy_request_data(4)
                session.backend.dummy_request_controlled_frames(4)
                session.backend.dummy_wait_process()

                let out1 = output_port_1.dummy_dequeue_data(4)
                let out2 = output_port_2.dummy_dequeue_data(4)

                verify_eq(out1.map(o => Math.round(o)), [2, 4, 6, 8])
                verify_eq(out2.map(o => Math.round(o)), [8, 6, 4, 2])
                verify_throw(fx.active)

            })
        }

        function test_drywet_audio_monitor_output_volume() {
            run_case('test_drywet_audio_monitor_output_volume', () => {
                check_backend()
                reset()
                tut_control().monitor = true
                tut_control().mute = false
                tut_control().volume_dB = 6.0
                testcase.wait(50)

                input_port_1.dummy_queue_data([1, 2, 3, 4])
                input_port_2.dummy_queue_data([4, 3, 2, 1])
                output_port_1.dummy_request_data(4)
                output_port_2.dummy_request_data(4)
                session.backend.dummy_request_controlled_frames(4)
                session.backend.dummy_wait_process()

                let out1 = output_port_1.dummy_dequeue_data(4)
                let out2 = output_port_2.dummy_dequeue_data(4)

                verify_eq(out1.map(o => Math.round(o)), [2, 4, 6, 8])
                verify_eq(out2.map(o => Math.round(o)), [8, 6, 4, 2])
                verify_throw(fx.active)

            })
        }

        function test_drywet_audio_monitor_output_balance_left() {
            run_case('test_drywet_audio_monitor_balance_left', () => {
                check_backend()
                reset()
                tut_control().monitor = true
                tut_control().mute = false
                tut_control().output_balance = -1.0
                tut_control().volume_dB = 6.0
                testcase.wait(50)

                input_port_1.dummy_queue_data([1, 2, 3, 4])
                input_port_2.dummy_queue_data([4, 3, 2, 1])
                output_port_1.dummy_request_data(4)
                output_port_2.dummy_request_data(4)
                session.backend.dummy_request_controlled_frames(4)
                session.backend.dummy_wait_process()

                let out1 = output_port_1.dummy_dequeue_data(4)
                let out2 = output_port_2.dummy_dequeue_data(4)

                verify_eq(out1.map(o => Math.round(o)), [2, 4, 6, 8])
                verify_eq(out2.map(o => Math.round(o)), [0, 0, 0, 0])
                verify_throw(fx.active)

            })
        }

        function test_drywet_audio_monitor_output_balance_right() {
            run_case('test_drywet_audio_monitor_output_balance_right', () => {
                check_backend()
                reset()
                tut_control().monitor = true
                tut_control().mute = false
                tut_control().output_balance = 1.0
                tut_control().volume_dB = 6.0
                testcase.wait(50)

                input_port_1.dummy_queue_data([1, 2, 3, 4])
                input_port_2.dummy_queue_data([4, 3, 2, 1])
                output_port_1.dummy_request_data(4)
                output_port_2.dummy_request_data(4)
                session.backend.dummy_request_controlled_frames(4)
                session.backend.dummy_wait_process()

                let out1 = output_port_1.dummy_dequeue_data(4)
                let out2 = output_port_2.dummy_dequeue_data(4)

                verify_eq(out1.map(o => Math.round(o)), [0, 0, 0, 0])
                verify_eq(out2.map(o => Math.round(o)), [8, 6, 4, 2])
                verify_throw(fx.active)

            })
        }

        function test_drywet_audio_no_monitor() {
            run_case('test_drywet_audio_no_monitor', () => {
                check_backend()
                reset()
                tut_control().monitor = false
                tut_control().mute = false
                testcase.wait(50)

                input_port_1.dummy_queue_data([1, 2, 3, 4])
                input_port_2.dummy_queue_data([4, 3, 2, 1])
                output_port_1.dummy_request_data(4)
                output_port_2.dummy_request_data(4)
                session.backend.dummy_request_controlled_frames(4)
                session.backend.dummy_wait_process()

                let out1 = output_port_1.dummy_dequeue_data(4)
                let out2 = output_port_2.dummy_dequeue_data(4)

                verify_eq(out1, [0, 0, 0, 0])
                verify_eq(out2, [0, 0, 0, 0])
                verify_throw(!fx.active)

            })
        }

        function test_drywet_audio_monitor_mute() {
            run_case('test_drywet_audio_monitor_mute', () => {
                check_backend()
                reset()
                tut_control().monitor = true
                tut_control().mute = true
                testcase.wait(50)

                input_port_1.dummy_queue_data([1, 2, 3, 4])
                input_port_2.dummy_queue_data([4, 3, 2, 1])
                output_port_1.dummy_request_data(4)
                output_port_2.dummy_request_data(4)
                session.backend.dummy_request_controlled_frames(4)
                session.backend.dummy_wait_process()

                let out1 = output_port_1.dummy_dequeue_data(4)
                let out2 = output_port_2.dummy_dequeue_data(4)

                verify_eq(out1, [0, 0, 0, 0])
                verify_eq(out2, [0, 0, 0, 0])
                verify_throw(fx.active)

            })
        }
    }
}