import QtQuick 6.6
import QtTest 1.0
import ShoopDaLoop.PythonBackend

import ShoopConstants
import './testfilename.js' as TestFilename
import '..' 

ShoopTestFile {
    PythonBackend {
        id: backend
        update_interval_ms: 30
        client_name_hint: 'shoop'
        backend_type: ShoopConstants.AudioDriverType.Dummy
        driver_setting_overrides: ({})

        MidiControlPort {
            id: midi_control_port_in
            name_hint: "control_in"
            direction: ShoopConstants.PortDirection.Input
            lua_engine: null
            autoconnect_regexes: []
            may_open: true
        }

        MidiControlPort {
            id: midi_control_port_out
            name_hint: "control_out"
            direction: ShoopConstants.PortDirection.Output
            lua_engine: null
            autoconnect_regexes: []
            may_open: true
        }


        ShoopTestCase {
            name: 'MidiControlPort'
            filename : TestFilename.test_filename()
            when: backend.initialized || backend.backend_type == null

            testcase_deinit_fn: () => { backend.close() }

            test_fns: ({
                'test_autoconnect_internal_in_external_out': () => {
                    backend.dummy_remove_all_external_mock_ports();

                    midi_control_port_in.autoconnect_regexes = ['.*testport.*']
                    
                    backend.dummy_add_external_mock_port("my_testport_out", ShoopConstants.PortDirection.Output, ShoopConstants.PortDataType.Midi)

                    midi_control_port_in.autoconnect_update()

                    verify_eq(
                        midi_control_port_in.get_connections_state(),
                        {
                            'my_testport_out': true
                        }
                    )
                }, 
                'test_autoconnect_internal_in_external_in': () => {
                    backend.dummy_remove_all_external_mock_ports();

                    midi_control_port_in.autoconnect_regexes = ['.*testport.*']
                    
                    backend.dummy_add_external_mock_port("my_testport_in", ShoopConstants.PortDirection.Input, ShoopConstants.PortDataType.Midi)

                    midi_control_port_in.autoconnect_update()

                    verify_eq(
                        midi_control_port_in.get_connections_state(),
                        {}
                    )
                }, 
                'test_autoconnect_internal_out_external_out': () => {
                    backend.dummy_remove_all_external_mock_ports();

                    midi_control_port_out.autoconnect_regexes = ['.*testport.*']
                    
                    backend.dummy_add_external_mock_port("my_testport_out", ShoopConstants.PortDirection.Output, ShoopConstants.PortDataType.Midi)

                    midi_control_port_out.autoconnect_update()

                    verify_eq(
                        midi_control_port_out.get_connections_state(),
                        {}
                    )
                }, 
                'test_autoconnect_internal_out_external_in': () => {
                    backend.dummy_remove_all_external_mock_ports();

                    midi_control_port_out.autoconnect_regexes = ['.*testport.*']
                    
                    backend.dummy_add_external_mock_port("my_testport_in", ShoopConstants.PortDirection.Input, ShoopConstants.PortDataType.Midi)

                    midi_control_port_out.autoconnect_update()

                    verify_eq(
                        midi_control_port_out.get_connections_state(),
                        {
                            'my_testport_in': true
                        }
                    )
                }               
            })
        }
    }
}