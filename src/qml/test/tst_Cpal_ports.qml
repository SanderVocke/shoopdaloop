import QtQuick 6.6
import ShoopDaLoop.Rust
import './testfilename.js' as TestFilename
import '..'

ShoopTestFile {
    Backend {
        id: backend
        update_interval_ms: 10
        client_name_hint: 'shoop-cpal-qml-test'
        backend_type: ShoopRustConstants.AudioDriverType.CpalTest
        driver_setting_overrides: ({
            cpal_output_device: 'default',
            cpal_input_device: 'none',
            cpal_sample_rate: 0,
            cpal_buffer_size: 0,
            cpal_input_channels: 'all',
            cpal_output_channels: 'all',
            cpal_capture_ring_frames: 256,
            midir_input: 'none',
            midir_output: 'none'
        })
    }

    AudioPort {
        descriptor: ({
            'schema': 'audioport.1',
            'id': 'audio_out',
            'name_parts': ['audio', '_out'],
            'type': 'driver',
            'input_connectability': ['internal'],
            'output_connectability': ['external'],
            'gain': 1.0,
            'muted': false,
            'passthrough_muted': false,
            'internal_port_connections': [],
            'external_port_connections': [],
            'min_n_ringbuffer_samples': 0
        })

        is_internal: false
        id: audio_out
        backend: backend
    }

    ShoopTestCase {
        name: 'CpalPorts'
        filename : TestFilename.test_filename()
        when: true

        test_fns: ({
            'test_virtual_playback_ports_are_app_connectable': () => {
                wait(500)
                if(!backend.ready) {
                    skip('No usable CPAL output backend available')
                    return
                }
                wait_condition(() => audio_out.initialized, 2000, 'audio_out did not initialize')

                let state = audio_out.get_connections_state()
                let candidates = Object.keys(state).filter(name => name.startsWith('cpal:') && name.indexOf(':playback_') >= 0)
                verify_true(candidates.length > 0, `No CPAL playback ports in ${JSON.stringify(state)}`)
                let target = candidates[0]
                verify_eq(state[target], false)

                audio_out.connect_external_port(target)
                wait(100)
                state = audio_out.get_connections_state()
                verify_eq(state[target], true)

                audio_out.disconnect_external_port(target)
                wait(100)
                state = audio_out.get_connections_state()
                verify_eq(state[target], false)
            }
        })
    }
}
