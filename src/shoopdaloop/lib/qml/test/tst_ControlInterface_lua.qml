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
    initial_descriptor: GenerateSession.generate_default_session(app_metadata.version_string, 2)

    ShoopSessionTestCase {
        id: testcase
        name: 'ControlInterface'
        filename : TestFilename.test_filename()
        session: session
        when: session.control_interface.ready

        function master_loop() {
            return session.tracks[0].loops[0]
        }

        function other_loop() {
            return session.tracks[0].loops[1]
        }

        property bool done_imports: false
        function prepare_imports() {
            if (!done_imports) {
                scripting_engine.execute(`
declare_global('shoop_control', require('shoop_control'))
declare_global('shoop_coords', require('shoop_coords'))
declare_global('shoop_helpers', require('shoop_helpers'))
declare_global('shoop_format', require('shoop_format'))
`, null, 'MidiControl', true, true)
                done_imports = true
            }
        }

        function clear() {
            master_loop().clear()
            other_loop().clear()
            registries.state_registry.replace('sync_active', false)
            master_loop().deselect()
            other_loop().deselect()
            testcase.wait(100)
            verify_loop_cleared(master_loop())
            verify_loop_cleared(other_loop())
        }

        function do_eval(code) {
            prepare_imports()
            return scripting_engine.evaluate(
                code,
                null,
                'test',
                true,
                false
            )
        }

        function do_execute(code) {
            prepare_imports()
            scripting_engine.execute(
                code,
                null,
                'test',
                true,
                false
            )
        }

        function verify_eq_lua(a, b) {
            verify(do_eval(`
                return (function()
                    local stringify
                    stringify = function(e)
                        if type(e) == 'table' then
                            local result = '{'
                            for k, v in pairs(e) do
                                result = result .. '[' .. stringify(k) .. '] = ' .. stringify(v) .. ', '
                            end
                            return result .. '}'
                        else
                            return tostring(e)
                        end
                    end
                    local a = stringify(${a})
                    local b = stringify(${b})
                    local result = (a == b)
                    if not result then
                        print_error('Expected ' .. tostring(a) .. ' == ' .. tostring(b))
                    end
                    return result
                end)()
            `))
        }

        function test_loop_count() {
            run_case('test_loop_count', () => {
                check_backend()
                clear()
                
                verify_eq(do_eval('return shoop_control.loop_count({})'), 0)
                verify_eq(do_eval('return shoop_control.loop_count({-1, -1})'), 0)
                verify_eq(do_eval('return shoop_control.loop_count({0, 0})'), 1)
                verify_eq(do_eval('return shoop_control.loop_count({{0, 0}})'), 1)
                verify_eq(do_eval('return shoop_control.loop_count({{0, 0}, {0, 1}})'), 2)
            })
        }

        function test_loop_get_which_selected() {
            run_case('test_loop_get_which_selected', () => {
                check_backend()
                clear()

                verify_eq_lua('shoop_control.loop_get_which_selected()', '{}')
                master_loop().select()
                verify_eq_lua('shoop_control.loop_get_which_selected()', '{{0, 0}}')
                other_loop().select()
                verify_eq_lua('shoop_control.loop_get_which_selected()', '{{0, 0}, {0, 1}}')
            })
        }

        function test_loop_get_which_targeted() {
            run_case('test_loop_get_which_targeted', () => {
                check_backend()
                clear()
                
                verify_eq_lua('shoop_control.loop_get_which_targeted()', 'nil')
                master_loop().target()
                verify_eq_lua('shoop_control.loop_get_which_targeted()', '{0, 0}')
                other_loop().target()
                verify_eq_lua('shoop_control.loop_get_which_targeted()', '{0, 1}')
            })
        }

        function test_loop_get_mode() {
            run_case('test_loop_get_mode', () => {
                check_backend()
                clear()
                
                verify_eq_lua('shoop_control.loop_get_mode({0,0})', '{ shoop_control.constants.LoopMode_Stopped }')
                master_loop().transition(Types.LoopMode.Recording, 0, false)
                wait(50)
                verify_eq_lua('shoop_control.loop_get_mode({0,0})', '{ shoop_control.constants.LoopMode_Recording }')
                verify_eq_lua('shoop_control.loop_get_mode({0,1})', '{ shoop_control.constants.LoopMode_Stopped }')
            })
        }

        function test_loop_transition() {
            run_case('test_loop_transition', () => {
                check_backend()
                clear()

                verify_eq(master_loop().mode, Types.LoopMode.Stopped)
                do_execute('shoop_control.loop_transition({0,0}, shoop_control.constants.LoopMode_Recording, 0)')
                wait(50)
                verify_eq(master_loop().mode, Types.LoopMode.Recording)
                verify_eq(other_loop().mode, Types.LoopMode.Stopped)
                do_execute('shoop_control.loop_transition({0,1}, shoop_control.constants.LoopMode_Recording, 0)')
                wait(50)
                verify_eq(master_loop().mode, Types.LoopMode.Recording)
                verify_eq(other_loop().mode, Types.LoopMode.Recording)
            })
        }

        function test_loop_set_get_volume() {
            run_case('test_loop_set_get_volume', () => {
                check_backend()
                clear()
                
                do_execute('shoop_control.loop_set_volume({0,0}, 1.0)')
                verify_eq_lua('shoop_control.loop_get_volume({0,0})', '1.0')
                do_execute('shoop_control.loop_set_volume({0,0}, 0.5)')
                verify_eq_lua('shoop_control.loop_get_volume({0,0})', '0.5')
            })
        }

        // function test_loop_set_get_volume_slider() {
        //     run_case('test_loop_set_get_volume_slider', () => {
        //         check_backend()
        //         clear()
                
        //         do_execute('shoop_control.loop_set_volume_slider({0,0}, 1.0)')
        //         verify_eq_lua('shoop_control.loop_get_volume_slider({0,0})', '1.0')
        //         do_execute('shoop_control.loop_set_volume_slider({0,0}, 0.5)')
        //         verify_eq_lua('shoop_control.loop_get_volume_slider({0,0})', '0.5')
        //     })
        // }

        // function test_loop_set_get_balance() {
        //     run_case('test_loop_set_get_balance', () => {
        //         check_backend()
        //         clear()

        //         do_execute('shoop_control.loop_set_balance({0,0}, 1.0)')
        //         verify_eq_lua('shoop_control.loop_get_balance({0,0})', '1.0')
        //         do_execute('shoop_control.loop_set_balance({0,0}, 0.5)')
        //         verify_eq_lua('shoop_control.loop_get_balance({0,0})', '0.5')
        //     })
        // }

        // function test_loop_record_n() {
        //     run_case('test_loop_record_n', () => {
        //         check_backend()
        //         clear()
        //         verify(false)
        //     })
        // }

        // function test_loop_record_with_targeted() {
        //     run_case('test_loop_record_with_targeted', () => {
        //         check_backend()
        //         clear()
        //         verify(false)
        //     })
        // }

        // function test_loop_select() {
        //     run_case('test_loop_select', () => {
        //         check_backend()
        //         clear()
        //         verify(false)
        //     })
        // }

        // function test_loop_target() {
        //     run_case('test_loop_target', () => {
        //         check_backend()
        //         clear()
        //         verify(false)
        //     })
        // }

        // function test_loop_untarget_all() {
        //     run_case('test_loop_untarget_all', () => {
        //         check_backend()
        //         clear()
        //         verify(false)
        //     })
        // }

        // function test_loop_toggle_selected() {
        //     run_case('test_loop_toggle_selected', () => {
        //         check_backend()
        //         clear()
        //         verify(false)
        //     })
        // }

        // function test_loop_toggle_targeted() {
        //     run_case('test_loop_toggle_targeted', () => {
        //         check_backend()
        //         clear()
        //         verify(false)
        //     })
        // }

        // function test_port_get_volume() {
        //     run_case('test_port_get_volume', () => {
        //         check_backend()
        //         clear()
        //         verify(false)
        //     })
        // }

        // function test_port_get_muted() {
        //     run_case('test_port_get_muted', () => {
        //         check_backend()
        //         clear()
        //         verify(false)
        //     })
        // }

        // function test_port_get_input_muted() {
        //     run_case('test_port_get_input_muted', () => {
        //         check_backend()
        //         clear()
        //         verify(false)
        //     })
        // }

        // function test_port_mute() {
        //     run_case('test_port_mute', () => {
        //         check_backend()
        //         clear()
        //         verify(false)
        //     })
        // }

        // function test_port_mute_input() {
        //     run_case('test_port_mute_input', () => {
        //         check_backend()
        //         clear()
        //         verify(false)
        //     })
        // }

        // function test_port_unmute() {
        //     run_case('test_port_unmute', () => {
        //         check_backend()
        //         clear()
        //         verify(false)
        //     })
        // }

        // function test_port_unmute_input() {
        //     run_case('test_port_unmute_input', () => {
        //         check_backend()
        //         clear()
        //         verify(false)
        //     })
        // }

        // function test_port_set_volume() {
        //     run_case('test_port_set_volume', () => {
        //         check_backend()
        //         clear()
        //         verify(false)
        //     })
        // }
    }
}