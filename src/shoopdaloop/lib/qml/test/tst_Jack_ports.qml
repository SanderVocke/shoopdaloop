import QtQuick 6.3
import QtTest 1.0
import ShoopDaLoop.PythonBackend
import ShoopDaLoop.PythonDummyJackTestServer

import '../../generated/types.js' as Types
import './testfilename.js' as TestFilename
import '..'

PythonDummyJackTestServer {
    id: server

    Backend {
        id: backend
        update_interval_ms: 30
        client_name_hint: 'shoop'
        backend_type: Types.BackendType.Jack
        backend_argstring: server.get_server_name()

        AudioPort {
            descriptor: ({
                'schema': 'audioport.1',
                'id': 'audio_in',
                'name_parts': ['audio', '_in'],
                'direction': 'input',
                'volume': 1.0,
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
                'volume': 1.0,
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
            when: audio_in.initialized

            function test_backend() {
                run_case('test_available_ports', () => {
                    verify(backend.initialized)
                    wait(100)

                    server.ensure_test_port('test1', 'audio_out', true, false)
                    server.ensure_test_port('test1', 'audio_in', true, true)
                    server.ensure_test_port('test2', 'audio_out', true, false)
                    server.ensure_test_port('test2', 'audio_in', true, true)
                    server.ensure_test_port('test1', 'midi_out', false, false)
                    server.ensure_test_port('test1', 'midi_in', false, true)
                    server.ensure_test_port('test2', 'midi_out', false, false)
                    server.ensure_test_port('test2', 'midi_in', false, true)

                    wait(100)

                    // verify_eq(audio_in.get_connections_state(), {
                    //     'test1:audio_out': false,
                    //     'test2:audio_out': false,
                    //     'shoop:audio_out': false
                    // })
                    // verify_eq(audio_out.get_connections_state(), {
                    //     'test1:audio_in': false,
                    //     'test2:audio_in': false,
                    //     'shoop:audio_in': false
                    // })
                    // verify_eq(midi_in.get_connections_state(), {
                    //     'test1:midi_out': false,
                    //     'test2:midi_out': false,
                    //     'shoop:midi_out': false
                    // })
                    // verify_eq(midi_out.get_connections_state(), {
                    //     'test1:midi_in': false,
                    //     'test2:midi_in': false,
                    //     'shoop:midi_in': false
                    // })
                })
            }
        }
    }
}