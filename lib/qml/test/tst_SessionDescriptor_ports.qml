import QtQuick 2.3
import QtTest 1.0
import Backend

import '../../../build/types.js' as Types
import '../../generate_session.js' as GenerateSession
import '..'

Backend {
    id: backend
    update_interval_ms: 30
    client_name_hint: 'ShoopDaLoop'
    backend_type: Types.BackendType.Dummy

    Session {
        id: session
        anchors.fill: parent
        initial_descriptor: ({
            'schema': 'session.1',
            'tracks': [],
            'ports': [
                {
                    'schema': 'audioport.1',
                    'id': 'an_audio_input',
                    'name_parts': ['audio_', 'input'],
                    'direction': 'input',
                    'volume': 1.0
                },
                {
                    'schema': 'audioport.1',
                    'id': 'an_audio_output',
                    'name_parts': ['audio_', 'output'],
                    'direction': 'output',
                    'volume': 1.0
                },
                {
                    'schema': 'midiport.1',
                    'id': 'a_midi_input',
                    'name_parts': ['midi_', 'input'],
                    'direction': 'input'
                },
                {
                    'schema': 'midiport.1',
                    'id': 'a_midi_output',
                    'name_parts': ['midi_', 'output'],
                    'direction': 'output',
                    'volume': 1.0
                },
            ]
        })
    }

    TestCase {
        when: session.loaded
        
        function test_session_descriptor_ports() {
            verify(backend.initialized)
        }

        function cleanupTestCase() { backend.close() }
    }
}