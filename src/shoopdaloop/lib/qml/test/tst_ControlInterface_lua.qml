import QtQuick 6.3
import QtTest 1.0
import ShoopDaLoop.PythonBackend

import './testDeepEqual.js' as TestDeepEqual
import ShoopConstants
import '../../generate_session.js' as GenerateSession
import './testfilename.js' as TestFilename
import '..'

ShoopTestFile {
    Session {
        id: session

        anchors.fill: parent
        initial_descriptor: {
            var sync_track = GenerateSession.generate_default_track("Sync", 2, 'sync', true, 'sync_loop', 0, 0, 2, false, false, false, undefined)
            var extra_track = GenerateSession.generate_default_track("Other", 2, 'other', false, 'other_loop', 0, 0, 2, false, false, false, undefined)
            return GenerateSession.generate_session(app_metadata.version_string, null, [sync_track, extra_track], [], [],
            [], [])
        }

        LuaEngine {
            id: lua_engine
            ready: false
            function update() {
                if (session.control_interface) {
                    create_lua_qobject_interface_as_global('__shoop_control_interface', session.control_interface)
                    ready = true
                }
            }
            Component.onCompleted: update()
            Component.onDestruction: session.control_interface.unregister_lua_engine(lua_engine)
        }
        Connections {
            target: session
            function onControl_interfaceChanged() { lua_engine.update() }
        }

        ShoopSessionTestCase {
            id: testcase
            name: 'ControlInterface'
            filename : TestFilename.test_filename()
            session: session
            when: lua_engine.ready && registries.state_registry && loop_at(0,0) && loop_at(0,1) && loop_at(1,0) && loop_at(1,1)

            function loop_at(track, idx) {
                if (session.tracks.length > track && session.tracks[track].loops.length > idx) {
                    return session.tracks[track].loops[idx]
                }
                return null
            }

            property bool done_imports: false
            function prepare_imports() {
                if (!done_imports) {
                    lua_engine.execute(`
    shoop_control = require('shoop_control')
    shoop_coords = require('shoop_coords')
    shoop_helpers = require('shoop_helpers')
    shoop_format = require('shoop_format')
    `, 'MidiControl', true, true)
                    done_imports = true
                }
            }

            function clear() {
                loop_at(0,0).create_backend_loop()
                loop_at(0,1).create_backend_loop()
                loop_at(1,0).create_backend_loop()
                loop_at(1,1).create_backend_loop()
                loop_at(0,0).clear()
                loop_at(0,1).clear()
                loop_at(1,0).clear()
                loop_at(1,1).clear()
                registries.state_registry.replace('sync_active', false)
                loop_at(0,0).deselect()
                loop_at(0,1).deselect()
                loop_at(1,0).deselect()
                loop_at(1,1).deselect()
                testcase.wait_updated(session.backend)
                verify_loop_cleared(loop_at(0,0))
                verify_loop_cleared(loop_at(0,1))
                verify_loop_cleared(loop_at(1,0))
                verify_loop_cleared(loop_at(1,1))
            }

            function do_eval(code) {
                prepare_imports()
                return lua_engine.evaluate(
                    code,
                    'test',
                    true,
                    false
                )
            }

            function do_execute(code) {
                prepare_imports()
                lua_engine.execute(
                    code,
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

            test_fns: ({

                'test_loop_count': () => {
                    check_backend()
                    clear()
                    
                    verify_eq(do_eval('return shoop_control.loop_count({})'), 0)
                    verify_eq(do_eval('return shoop_control.loop_count({-1, -1})'), 0)
                    verify_eq(do_eval('return shoop_control.loop_count({0, 0})'), 1)
                    verify_eq(do_eval('return shoop_control.loop_count({{0, 0}})'), 1)
                    verify_eq(do_eval('return shoop_control.loop_count({{0, 0}, {0, 1}})'), 2)
                },

                'test_loop_get_which_selected': () => {
                    check_backend()
                    clear()

                    verify_eq_lua('shoop_control.loop_get_which_selected()', '{}')
                    loop_at(0,0).select()
                    verify_eq_lua('shoop_control.loop_get_which_selected()', '{{0, 0}}')
                    loop_at(0,1).select()
                    verify_eq_lua('shoop_control.loop_get_which_selected()', '{{0, 0}, {0, 1}}')
                },

                'test_loop_get_which_targeted': () => {
                    check_backend()
                    clear()
                    
                    verify_eq_lua('shoop_control.loop_get_which_targeted()', 'nil')
                    loop_at(0,0).target()
                    verify_eq_lua('shoop_control.loop_get_which_targeted()', '{0, 0}')
                    loop_at(0,1).target()
                    verify_eq_lua('shoop_control.loop_get_which_targeted()', '{0, 1}')
                },

                'test_loop_get_mode': () => {
                    check_backend()
                    clear()
                    
                    verify_eq_lua('shoop_control.loop_get_mode({0,0})', '{ shoop_control.constants.LoopMode_Stopped }')
                    loop_at(0,0).transition(ShoopConstants.LoopMode.Recording, 0, false)
                    testcase.wait_updated(session.backend)
                    verify_eq_lua('shoop_control.loop_get_mode({0,0})', '{ shoop_control.constants.LoopMode_Recording }')
                    verify_eq_lua('shoop_control.loop_get_mode({0,1})', '{ shoop_control.constants.LoopMode_Stopped }')
                    verify_eq_lua('shoop_control.loop_get_mode({{1,0},{0,0}})', '{ shoop_control.constants.LoopMode_Stopped, shoop_control.constants.LoopMode_Recording }')
                    verify_eq_lua('shoop_control.loop_get_mode({})', '{}')
                },

                'test_loop_transition': () => {
                    check_backend()
                    clear()

                    verify_eq(loop_at(0,0).mode, ShoopConstants.LoopMode.Stopped)
                    do_execute('shoop_control.loop_transition({0,0}, shoop_control.constants.LoopMode_Recording, 0)')
                    testcase.wait_updated(session.backend)
                    verify_eq(loop_at(0,0).mode, ShoopConstants.LoopMode.Recording)
                    verify_eq(loop_at(0,1).mode, ShoopConstants.LoopMode.Stopped)
                    do_execute('shoop_control.loop_transition({0,1}, shoop_control.constants.LoopMode_Recording, 0)')
                    testcase.wait_updated(session.backend)
                    verify_eq(loop_at(0,0).mode, ShoopConstants.LoopMode.Recording)
                    verify_eq(loop_at(0,1).mode, ShoopConstants.LoopMode.Recording)
                },

                'test_loop_set_get_gain': () => {
                    check_backend()
                    clear()
                    
                    do_execute('shoop_control.loop_set_gain({0,0}, 1.0)')
                    do_execute('shoop_control.loop_set_gain({1,0}, 1.0)')
                    verify_eq_lua('shoop_control.loop_get_gain({0,0})', '{1.0}')
                    do_execute('shoop_control.loop_set_gain({0,0}, 0.5)')
                    verify_eq_lua('shoop_control.loop_get_gain({0,0})', '{0.5}')
                    verify_eq_lua('shoop_control.loop_get_gain({{1,0},{0,0}})', '{1.0, 0.5}')
                },

                'test_loop_set_get_gain_fader': () => {
                    check_backend()
                    clear()
                    
                    do_execute('shoop_control.loop_set_gain_fader({0,0}, 1.0)')
                    verify_eq_lua('shoop_control.loop_get_gain_fader({0,0})', '{1.0}')
                    do_execute('shoop_control.loop_set_gain_fader({0,0}, 0.5)')
                    verify_eq_lua('shoop_control.loop_get_gain_fader({0,0})', '{0.5}')
                    do_execute('shoop_control.loop_set_gain_fader({0,0}, 2.0)')
                    verify_eq_lua('shoop_control.loop_get_gain_fader({0,0})', '{1.0}')
                    do_execute('shoop_control.loop_set_gain_fader({0,0}, -1.0)')
                    verify_eq_lua('shoop_control.loop_get_gain_fader({0,0})', '{0.0}')
                },

                'test_loop_set_get_balance': () => {
                    check_backend()
                    clear()

                    do_execute('shoop_control.loop_set_balance({0,0}, 1.0)')
                    do_execute('shoop_control.loop_set_balance({1,0}, 1.0)')
                    verify_eq_lua('shoop_control.loop_get_balance({0,0})', '{1.0}')
                    do_execute('shoop_control.loop_set_balance({0,0}, 0.5)')
                    verify_eq_lua('shoop_control.loop_get_balance({0,0})', '{0.5}')
                    verify_eq_lua('shoop_control.loop_get_balance({{0,0},{1,0}})', '{0.5, 1.0}')
                },

                // TODO: harder to test because this requires loops to
                // trigger each other
                // 'test_loop_record_n': () => {
                //         check_backend()
                //         clear()
                //         verify(false)
                //     })
                // }

                // TODO: harder to test because this requires loops to
                // trigger each other
                // 'test_loop_record_with_targeted': () => {
                //         check_backend()
                //         clear()
                //         verify(false)
                //     })
                // }

                'test_loop_select': () => {
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
                },

                'test_loop_target': () => {
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
                },

                'test_loop_clear': () => {
                    check_backend()
                    clear()

                    loop_at(0,0).set_length(100)
                    loop_at(0,1).set_length(100)
                    testcase.wait_updated(session.backend)
                    
                    do_execute('shoop_control.loop_clear({0,0})')
                    testcase.wait_updated(session.backend)

                    verify_loop_cleared(loop_at(0,0))
                    verify(loop_at(0,1).length > 0)

                    do_execute('shoop_control.loop_clear({0,1})')
                    testcase.wait_updated(session.backend)

                    verify_loop_cleared(loop_at(0,0))
                    verify_loop_cleared(loop_at(0,1))

                    loop_at(0,0).set_length(100)
                    loop_at(0,1).set_length(100)
                    loop_at(1,0).set_length(100)
                    loop_at(1,1).set_length(100)
                    testcase.wait_updated(session.backend)

                    do_execute('shoop_control.loop_clear({{0,0}, {0,1}, {1,0}, {1,1}})')
                    testcase.wait_updated(session.backend)

                    verify_loop_cleared(loop_at(0,0))
                    verify_loop_cleared(loop_at(0,1))
                    verify_loop_cleared(loop_at(1,0))
                    verify_loop_cleared(loop_at(1,1))
                },

                'test_loop_get_all': () => {
                    check_backend()
                    clear()
                    
                    verify_eq_lua('shoop_control.loop_get_all()', '{{0,0}, {0,1}, {1,0}, {1,1}}')
                },

                'test_loop_get_by_mode': () => {
                    check_backend()
                    clear()
                    
                    verify_eq_lua('shoop_control.loop_get_by_mode(shoop_control.constants.LoopMode_Stopped)', '{{0,0}, {0,1}, {1,0}, {1,1}}')
                    loop_at(0,0).transition(ShoopConstants.LoopMode.Recording, 0, false)
                    testcase.wait_updated(session.backend)
                    verify_eq_lua('shoop_control.loop_get_by_mode(shoop_control.constants.LoopMode_Stopped)', '{{0,1},{1,0},{1,1}}')
                    verify_eq_lua('shoop_control.loop_get_by_mode(shoop_control.constants.LoopMode_Recording)', '{{0,0}}')
                },

                'test_loop_get_by_track': () => {
                    check_backend()
                    clear()
                    
                    verify_eq_lua('shoop_control.loop_get_by_track(0)', '{{0,0}, {0,1}}')
                    verify_eq_lua('shoop_control.loop_get_by_track(1)', '{{1,0}, {1,1}}')
                },

                'test_loop_get_length': () => {
                    check_backend()
                    clear()
                    
                    verify_eq_lua('shoop_control.loop_get_length({0,0})', '{0}')

                    loop_at(1,1).set_length(100)
                    testcase.wait_updated(session.backend)

                    verify_eq_lua('shoop_control.loop_get_length({1,1})', '{100}')
                },

                'test_track_set_get_gain': () => {
                    check_backend()
                    clear()
                    
                    do_execute('shoop_control.track_set_gain(0, 1.0)')
                    do_execute('shoop_control.track_set_gain(1, 1.0)')
                    verify_eq_lua('shoop_control.track_get_gain(0)', '{1.0}')
                    do_execute('shoop_control.track_set_gain(0, 0.5)')
                    verify_eq_lua('shoop_control.track_get_gain(0)', '{0.5}')
                    verify_eq_lua('shoop_control.track_get_gain({1,0})', '{1.0, 0.5}')
                },

                'test_track_set_get_gain_fader': () => {
                    check_backend()
                    clear()
                    
                    do_execute('shoop_control.track_set_gain_fader(0, 1.0)')
                    verify_eq_lua('shoop_control.track_get_gain_fader(0)', '{1.0}')
                    do_execute('shoop_control.track_set_gain_fader(0, 0.5)')
                    verify_eq_lua('shoop_control.track_get_gain_fader(0)', '{0.5}')
                    do_execute('shoop_control.track_set_gain_fader(0, 2.0)')
                    verify_eq_lua('shoop_control.track_get_gain_fader(0)', '{1.0}')
                    do_execute('shoop_control.track_set_gain_fader(0, -1.0)')
                    verify_eq_lua('shoop_control.track_get_gain_fader(0)', '{0.0}')
                },

                'test_track_set_get_input_gain': () => {
                    check_backend()
                    clear()
                    
                    do_execute('shoop_control.track_set_input_gain(0, 1.0)')
                    do_execute('shoop_control.track_set_input_gain(1, 1.0)')
                    verify_eq_lua('shoop_control.track_get_input_gain(0)', '{1.0}')
                    do_execute('shoop_control.track_set_input_gain(0, 0.5)')
                    verify_eq_lua('shoop_control.track_get_input_gain(0)', '{0.5}')
                    verify_eq_lua('shoop_control.track_get_input_gain({1,0})', '{1.0, 0.5}')
                },

                'test_track_set_get_input_gain_fader': () => {
                    check_backend()
                    clear()
                    
                    do_execute('shoop_control.track_set_input_gain_fader(0, 1.0)')
                    verify_eq_lua('shoop_control.track_get_input_gain_fader(0)', '{1.0}')
                    do_execute('shoop_control.track_set_input_gain_fader(0, 0.5)')
                    verify_eq_lua('shoop_control.track_get_input_gain_fader(0)', '{0.5}')
                    do_execute('shoop_control.track_set_input_gain_fader(0, 2.0)')
                    verify_eq_lua('shoop_control.track_get_input_gain_fader(0)', '{1.0}')
                    do_execute('shoop_control.track_set_input_gain_fader(0, -1.0)')
                    verify_eq_lua('shoop_control.track_get_input_gain_fader(0)', '{0.0}')
                },

                'test_track_set_get_muted': () => {
                    check_backend()
                    clear()
                    
                    do_execute('shoop_control.track_set_muted(0, true)')
                    do_execute('shoop_control.track_set_muted(1, true)')
                    verify_eq_lua('shoop_control.track_get_muted(0)', '{true}')
                    verify_eq_lua('shoop_control.track_get_muted({1,0})', '{true, true}')
                    do_execute('shoop_control.track_set_muted(0, false)')
                    verify_eq_lua('shoop_control.track_get_muted(0)', '{false}')
                    verify_eq_lua('shoop_control.track_get_muted({1,0})', '{true, false}')
                },

                'test_track_set_get_input_muted': () => {
                    check_backend()
                    clear()
                    
                    do_execute('shoop_control.track_set_input_muted(0, true)')
                    do_execute('shoop_control.track_set_input_muted(1, true)')
                    verify_eq_lua('shoop_control.track_get_input_muted(0)', '{true}')
                    verify_eq_lua('shoop_control.track_get_input_muted({1,0})', '{true, true}')
                    do_execute('shoop_control.track_set_input_muted(0, false)')
                    verify_eq_lua('shoop_control.track_get_input_muted(0)', '{false}')
                    verify_eq_lua('shoop_control.track_get_input_muted({1,0})', '{true, false}')
                },
            })
        }
    }
}