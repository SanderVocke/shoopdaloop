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
        var master_track = GenerateSession.generate_default_track("Master", 2, 'master', true, 'master_loop', 0, 0, 2, false, false, false, undefined)
        var extra_track = GenerateSession.generate_default_track("Other", 2, 'other', false, 'other_loop', 0, 0, 2, false, false, false, undefined)
        return GenerateSession.generate_session(app_metadata.version_string, [master_track, extra_track], [], [],
        [], [])
    }

    ShoopSessionTestCase {
        id: testcase
        name: 'ControlInterface'
        filename : TestFilename.test_filename()
        session: session
        when: session.control_interface.ready && registries.state_registry

        function loop_at(track, idx) {
            return session.tracks[track].loops[idx]
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
            loop_at(0,0).clear()
            loop_at(0,1).clear()
            loop_at(1,0).clear()
            loop_at(1,1).clear()
            registries.state_registry.replace('sync_active', false)
            loop_at(0,0).deselect()
            loop_at(0,1).deselect()
            loop_at(1,0).deselect()
            loop_at(1,1).deselect()
            testcase.wait(50)
            verify_loop_cleared(loop_at(0,0))
            verify_loop_cleared(loop_at(0,1))
            verify_loop_cleared(loop_at(1,0))
            verify_loop_cleared(loop_at(1,1))
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
                loop_at(0,0).select()
                verify_eq_lua('shoop_control.loop_get_which_selected()', '{{0, 0}}')
                loop_at(0,1).select()
                verify_eq_lua('shoop_control.loop_get_which_selected()', '{{0, 0}, {0, 1}}')
            })
        }

        function test_loop_get_which_targeted() {
            run_case('test_loop_get_which_targeted', () => {
                check_backend()
                clear()
                
                verify_eq_lua('shoop_control.loop_get_which_targeted()', 'nil')
                loop_at(0,0).target()
                verify_eq_lua('shoop_control.loop_get_which_targeted()', '{0, 0}')
                loop_at(0,1).target()
                verify_eq_lua('shoop_control.loop_get_which_targeted()', '{0, 1}')
            })
        }

        function test_loop_get_mode() {
            run_case('test_loop_get_mode', () => {
                check_backend()
                clear()
                
                verify_eq_lua('shoop_control.loop_get_mode({0,0})', '{ shoop_control.constants.LoopMode_Stopped }')
                loop_at(0,0).transition(Types.LoopMode.Recording, 0, false)
                wait(50)
                verify_eq_lua('shoop_control.loop_get_mode({0,0})', '{ shoop_control.constants.LoopMode_Recording }')
                verify_eq_lua('shoop_control.loop_get_mode({0,1})', '{ shoop_control.constants.LoopMode_Stopped }')
                verify_eq_lua('shoop_control.loop_get_mode({{1,0},{0,0}})', '{ shoop_control.constants.LoopMode_Stopped, shoop_control.constants.LoopMode_Recording }')
                verify_eq_lua('shoop_control.loop_get_mode({})', '{}')
            })
        }

        function test_loop_transition() {
            run_case('test_loop_transition', () => {
                check_backend()
                clear()

                verify_eq(loop_at(0,0).mode, Types.LoopMode.Stopped)
                do_execute('shoop_control.loop_transition({0,0}, shoop_control.constants.LoopMode_Recording, 0)')
                wait(50)
                verify_eq(loop_at(0,0).mode, Types.LoopMode.Recording)
                verify_eq(loop_at(0,1).mode, Types.LoopMode.Stopped)
                do_execute('shoop_control.loop_transition({0,1}, shoop_control.constants.LoopMode_Recording, 0)')
                wait(50)
                verify_eq(loop_at(0,0).mode, Types.LoopMode.Recording)
                verify_eq(loop_at(0,1).mode, Types.LoopMode.Recording)
            })
        }

        function test_loop_set_get_volume() {
            run_case('test_loop_set_get_volume', () => {
                check_backend()
                clear()
                
                do_execute('shoop_control.loop_set_volume({0,0}, 1.0)')
                do_execute('shoop_control.loop_set_volume({1,0}, 1.0)')
                verify_eq_lua('shoop_control.loop_get_volume({0,0})', '{1.0}')
                do_execute('shoop_control.loop_set_volume({0,0}, 0.5)')
                verify_eq_lua('shoop_control.loop_get_volume({0,0})', '{0.5}')
                verify_eq_lua('shoop_control.loop_get_volume({{1,0},{0,0}})', '{1.0, 0.5}')
            })
        }

        function test_loop_set_get_volume_slider() {
            run_case('test_loop_set_get_volume_slider', () => {
                check_backend()
                clear()
                
                do_execute('shoop_control.loop_set_volume_slider({0,0}, 1.0)')
                verify_eq_lua('shoop_control.loop_get_volume_slider({0,0})', '{1.0}')
                do_execute('shoop_control.loop_set_volume_slider({0,0}, 0.5)')
                verify_eq_lua('shoop_control.loop_get_volume_slider({0,0})', '{0.5}')
                do_execute('shoop_control.loop_set_volume_slider({0,0}, 2.0)')
                verify_eq_lua('shoop_control.loop_get_volume_slider({0,0})', '{1.0}')
                do_execute('shoop_control.loop_set_volume_slider({0,0}, -1.0)')
                verify_eq_lua('shoop_control.loop_get_volume_slider({0,0})', '{0.0}')
            })
        }

        function test_loop_set_get_balance() {
            run_case('test_loop_set_get_balance', () => {
                check_backend()
                clear()

                do_execute('shoop_control.loop_set_balance({0,0}, 1.0)')
                do_execute('shoop_control.loop_set_balance({1,0}, 1.0)')
                verify_eq_lua('shoop_control.loop_get_balance({0,0})', '{1.0}')
                do_execute('shoop_control.loop_set_balance({0,0}, 0.5)')
                verify_eq_lua('shoop_control.loop_get_balance({0,0})', '{0.5}')
                verify_eq_lua('shoop_control.loop_get_balance({{0,0},{1,0}})', '{0.5, 1.0}')
            })
        }

        // TODO: harder to test because this requires loops to
        // trigger each other
        // function test_loop_record_n() {
        //     run_case('test_loop_record_n', () => {
        //         check_backend()
        //         clear()
        //         verify(false)
        //     })
        // }

        // TODO: harder to test because this requires loops to
        // trigger each other
        // function test_loop_record_with_targeted() {
        //     run_case('test_loop_record_with_targeted', () => {
        //         check_backend()
        //         clear()
        //         verify(false)
        //     })
        // }

        function test_loop_select() {
            run_case('test_loop_select', () => {
                check_backend()
                clear()
                
                do_execute('shoop_control.loop_select({0,0}, true)')
                verify_eq_lua('shoop_control.loop_get_which_selected()', '{{0,0}}')
                do_execute('shoop_control.loop_select({0,1}, true)')
                verify_eq_lua('shoop_control.loop_get_which_selected()', '{{0,1}}')
                do_execute('shoop_control.loop_select({0,0}, false)')
                verify_eq_lua('shoop_control.loop_get_which_selected()', '{{0,0}, {0,1}}')
                do_execute('shoop_control.loop_select({}, true)')
                verify_eq_lua('shoop_control.loop_get_which_selected()', '{}')
                do_execute('shoop_control.loop_select({{0,0}, {0,1}}, false)')
                verify_eq_lua('shoop_control.loop_get_which_selected()', '{{0,0}, {0,1}}')
            })
        }

        function test_loop_target() {
            run_case('test_loop_target', () => {
                check_backend()
                clear()
                
                do_execute('shoop_control.loop_target({0,0})')
                verify_eq_lua('shoop_control.loop_get_which_targeted()', '{0,0}')
                do_execute('shoop_control.loop_target({0,1})')
                verify_eq_lua('shoop_control.loop_get_which_targeted()', '{0,1}')
                do_execute('shoop_control.loop_target({})')
                verify_eq_lua('shoop_control.loop_get_which_targeted()', 'nil')
                do_execute('shoop_control.loop_target(nil)')
                verify_eq_lua('shoop_control.loop_get_which_targeted()', 'nil')
            })
        }

        function test_loop_clear() {
            run_case('test_loop_clear', () => {
                check_backend()
                clear()

                loop_at(0,0).set_length(100)
                loop_at(0,1).set_length(100)
                wait(50)
                
                do_execute('shoop_control.loop_clear({0,0})')
                wait(50)

                verify_loop_cleared(loop_at(0,0))
                verify(loop_at(0,1).length > 0)

                do_execute('shoop_control.loop_clear({0,1})')
                wait(50)

                verify_loop_cleared(loop_at(0,0))
                verify_loop_cleared(loop_at(0,1))

                loop_at(0,0).set_length(100)
                loop_at(0,1).set_length(100)
                loop_at(1,0).set_length(100)
                loop_at(1,1).set_length(100)
                wait(50)

                do_execute('shoop_control.loop_clear({{0,0}, {0,1}, {1,0}, {1,1}})')
                wait(50)

                verify_loop_cleared(loop_at(0,0))
                verify_loop_cleared(loop_at(0,1))
                verify_loop_cleared(loop_at(1,0))
                verify_loop_cleared(loop_at(1,1))
            })
        }

        function test_loop_get_all() {
            run_case('test_loop_get_all', () => {
                check_backend()
                clear()
                
                verify_eq_lua('shoop_control.loop_get_all()', '{{0,0}, {0,1}, {1,0}, {1,1}}')
            })
        }

        function test_loop_get_by_mode() {
            run_case('test_loop_get_by_mode', () => {
                check_backend()
                clear()
                
                verify_eq_lua('shoop_control.loop_get_by_mode(shoop_control.constants.LoopMode_Stopped)', '{{0,0}, {0,1}, {1,0}, {1,1}}')
                loop_at(0,0).transition(Types.LoopMode.Recording, 0, false)
                wait(50)
                verify_eq_lua('shoop_control.loop_get_by_mode(shoop_control.constants.LoopMode_Stopped)', '{{0,1},{1,0},{1,1}}')
                verify_eq_lua('shoop_control.loop_get_by_mode(shoop_control.constants.LoopMode_Recording)', '{{0,0}}')
            })
        }

        function test_loop_get_by_track() {
            run_case('test_loop_get_by_track', () => {
                check_backend()
                clear()
                
                verify_eq_lua('shoop_control.loop_get_by_track(0)', '{{0,0}, {0,1}}')
                verify_eq_lua('shoop_control.loop_get_by_track(1)', '{{1,0}, {1,1}}')
            })
        }

        function test_loop_get_length() {
            run_case('test_loop_get_length', () => {
                check_backend()
                clear()
                
                verify_eq_lua('shoop_control.loop_get_length({0,0})', '{0}')

                loop_at(1,1).set_length(100)
                wait(50)

                verify_eq_lua('shoop_control.loop_get_length({1,1})', '{100}')
            })
        }

        function test_track_set_get_volume() {
            run_case('test_track_set_get_volume', () => {
                check_backend()
                clear()
                
                do_execute('shoop_control.track_set_volume(0, 1.0)')
                do_execute('shoop_control.track_set_volume(1, 1.0)')
                verify_eq_lua('shoop_control.track_get_volume(0)', '{1.0}')
                do_execute('shoop_control.track_set_volume(0, 0.5)')
                verify_eq_lua('shoop_control.track_get_volume(0)', '{0.5}')
                verify_eq_lua('shoop_control.track_get_volume({1,0})', '{1.0, 0.5}')
            })
        }

        function test_track_set_get_volume_slider() {
            run_case('test_track_set_get_volume_slider', () => {
                check_backend()
                clear()
                
                do_execute('shoop_control.track_set_volume_slider(0, 1.0)')
                verify_eq_lua('shoop_control.track_get_volume_slider(0)', '{1.0}')
                do_execute('shoop_control.track_set_volume_slider(0, 0.5)')
                verify_eq_lua('shoop_control.track_get_volume_slider(0)', '{0.5}')
                do_execute('shoop_control.track_set_volume_slider(0, 2.0)')
                verify_eq_lua('shoop_control.track_get_volume_slider(0)', '{1.0}')
                do_execute('shoop_control.track_set_volume_slider(0, -1.0)')
                verify_eq_lua('shoop_control.track_get_volume_slider(0)', '{0.0}')
            })
        }

        // track_get_input_muted
        // track_get_input_volume
        // track_get_input_volume_slider
        // track_get_muted
        // track_get_volume
        // track_get_volume_slider
        // track_set_input_muted
        // track_set_input_volume
        // track_set_input_volume_slider
        // track_set_volume
        // track_set_volume_slider
    }
}