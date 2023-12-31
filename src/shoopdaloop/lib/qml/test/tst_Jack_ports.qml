import QtQuick 6.3
import QtTest 1.0
import ShoopDaLoop.PythonBackend

import ShoopConstants
import './testfilename.js' as TestFilename
import '..'

ShoopTestFile {
    Backend {
        id: backend
        update_interval_ms: 30
        client_name_hint: 'shoop'
        backend_type: backend_type_is_supported(ShoopConstants.AudioDriverType.JackTest) ?
                      ShoopConstants.AudioDriverType.JackTest : ShoopConstants.AudioDriverType.Dummy
        driver_setting_overrides: ({})

    AudioPort {
        descriptor: ({
            'schema': 'audioport.1',
            'id': 'audio_in',
            'name_parts': ['audio', '_in'],
            'direction': 'input',
            'gain': 1.0,
            'muted': false,
            'passthrough_muted': false,
            'passthrough_to': [],
            'external_port_connections': []
        })

        is_internal: false
        id: audio_in
    }
    AudioPort {
        descriptor: ({
            'schema': 'audioport.1',
            'id': 'audio_out',
            'name_parts': ['audio', '_out'],
            'direction': 'output',
            'gain': 1.0,
            'muted': false,
            'passthrough_muted': false,
            'passthrough_to': [],
            'external_port_connections': []
        })

            is_internal: false
            id: audio_out
        }
        MidiPort {
            descriptor: ({
                'schema': 'midiport.1',
                'id': 'midi_in',
                'name_parts': ['midi', '_in'],
                'direction': 'input',
                'muted': false,
                'passthrough_muted': false,
                'passthrough_to': [],
                'external_port_connections': []
            })

            is_internal: false
            id: midi_in
        }
        MidiPort {
            descriptor: ({
                'schema': 'midiport.1',
                'id': 'midi_out',
                'name_parts': ['midi', '_out'],
                'direction': 'output',
                'muted': false,
                'passthrough_muted': false,
                'passthrough_to': [],
                'external_port_connections': []
            })

            is_internal: false
            id: midi_out
        }

        ShoopTestCase {
            name: 'JackPorts'
            filename : TestFilename.test_filename()

            property bool skip_no_jack : !backend.backend_type_is_supported(ShoopConstants.AudioDriverType.JackTest)
            when: skip_no_jack || (audio_in.initialized && audio_out.initialized && midi_in.initialized && midi_out.initialized)

            test_fns: ({
                'test_available_ports': () => {
                    if(skip_no_jack) {
                        skip("Backend was built without Jack support")
                        return
                    }
                    verify(backend.initialized)

                    wait(100)

                    verify_eq(audio_in.get_connections_state(), {
                        'test_client_1:audio_out': false,
                        'test_client_2:audio_out': false,
                        'shoop:audio_out': false
                    })
                    verify_eq(audio_out.get_connections_state(), {
                        'test_client_1:audio_in': false,
                        'test_client_2:audio_in': false,
                        'shoop:audio_in': false
                    })
                    verify_eq(midi_in.get_connections_state(), {
                        'test_client_1:midi_out': false,
                        'test_client_2:midi_out': false,
                        'shoop:midi_out': false
                    })
                    verify_eq(midi_out.get_connections_state(), {
                        'test_client_1:midi_in': false,
                        'test_client_2:midi_in': false,
                        'shoop:midi_in': false
                    })

                    wait(100)

                    audio_in.connect_external_port('test_client_1:audio_out')
                    audio_out.connect_external_port('test_client_2:audio_in')
                    midi_in.connect_external_port('test_client_2:midi_out')
                    midi_out.connect_external_port('test_client_1:midi_in')

                    wait(100)

                    verify_eq(audio_in.get_connections_state(), {
                        'test_client_1:audio_out': true,
                        'test_client_2:audio_out': false,
                        'shoop:audio_out': false
                    })
                    verify_eq(audio_out.get_connections_state(), {
                        'test_client_1:audio_in': false,
                        'test_client_2:audio_in': true,
                        'shoop:audio_in': false
                    })
                    verify_eq(midi_in.get_connections_state(), {
                        'test_client_1:midi_out': false,
                        'test_client_2:midi_out': true,
                        'shoop:midi_out': false
                    })
                    verify_eq(midi_out.get_connections_state(), {
                        'test_client_1:midi_in': true,
                        'test_client_2:midi_in': false,
                        'shoop:midi_in': false
                    })

                    audio_in.disconnect_external_port('test_client_1:audio_out')
                    audio_out.disconnect_external_port('test_client_2:audio_in')
                    midi_in.disconnect_external_port('test_client_2:midi_out')
                    midi_out.disconnect_external_port('test_client_1:midi_in')

                    verify_eq(audio_in.get_connections_state(), {
                        'test_client_1:audio_out': false,
                        'test_client_2:audio_out': false,
                        'shoop:audio_out': false
                    })
                    verify_eq(audio_out.get_connections_state(), {
                        'test_client_1:audio_in': false,
                        'test_client_2:audio_in': false,
                        'shoop:audio_in': false
                    })
                    verify_eq(midi_in.get_connections_state(), {
                        'test_client_1:midi_out': false,
                        'test_client_2:midi_out': false,
                        'shoop:midi_out': false
                    })
                    verify_eq(midi_out.get_connections_state(), {
                        'test_client_1:midi_in': false,
                        'test_client_2:midi_in': false,
                        'shoop:midi_in': false
                    })
                }
            })
        }
    }
}