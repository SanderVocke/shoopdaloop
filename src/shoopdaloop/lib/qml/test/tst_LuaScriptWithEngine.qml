import QtQuick 6.6
import QtTest 1.0
import ShoopDaLoop.PythonBackend

import ShoopConstants
import '../../generate_session.js' as GenerateSession
import './testfilename.js' as TestFilename
import '..' 

ShoopTestFile {
    Session {
        id: session

        anchors.fill: parent
        initial_descriptor: {
            let track = GenerateSession.generate_default_track(
                "tut",
                1,
                "tut",
                false,
                "tut"
            )
            let _session = GenerateSession.generate_default_session(app_metadata.version_string, null, true, 1, 1, [track])
            return _session
        }

        Component {
            id: factory
            LuaScriptWithEngine {
                control_interface : session.control_interface
                script_code : `
                local shoop_control = require('shoop_control')
                local opened = false
                local connected = false
                local on_opened = function()
                    opened = true
                end
                local on_connected = function()
                    connected = true
                end

                shoop_control.auto_open_device_specific_midi_control_output(".*testport.*", on_opened, on_connected, 0)

                `
            }
        }

        ShoopSessionTestCase {
            name: 'Lua_autoconnect'
            session: session
            filename : TestFilename.test_filename()
            when: backend.initialized || backend.backend_type == null

            testcase_deinit_fn: () => { backend.close() }

            test_fns: ({
                'test_autoconnect': () => {
                    check_backend()
                    session.backend.dummy_remove_all_external_mock_ports();
                    verify_eq(Object.values(session.control_interface.midi_control_ports).length, 0)

                    // Create and run the script. Should be listening then for ports to connect to.
                    let script = factory.createObject(this, {})
                    wait_condition(() => script.ran)
                    verify_eq(script.listening, true)
                    verify_eq(Object.values(session.control_interface.midi_control_ports).length, 1)
                    let port = Object.values(session.control_interface.midi_control_ports)[0]
                    verify_eq(port.initialized, false)

                    backend.dummy_add_external_mock_port("my_testport", ShoopConstants.PortDirection.Input, ShoopConstants.PortDataType.Midi)
                    wait_condition(() => port.initialized)

                    script.stop()

                    verify_eq(Object.values(session.control_interface.midi_control_ports).length, 0)

                    script.start()

                    verify_eq(Object.values(session.control_interface.midi_control_ports).length, 1)
                    let port2 = Object.values(session.control_interface.midi_control_ports)[0]
                    wait_condition(() => port2.initialized)

                    script.destroy()
                },
            })
        }
    }
}