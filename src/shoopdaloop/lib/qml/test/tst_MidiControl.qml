import QtQuick 6.3

import '../../generated/types.js' as Types
import '../../midi.js' as Midi
import '../../midi_control.js' as MidiControl
import './testfilename.js' as TestFilename
import '..'

MidiControl {
    id: ctl
    when: itf.ready

    // A dummy control interface which logs calls for testing
    component Interface: LuaControlInterface {
        property var logged_calls: []
        function clear() { logged_calls = [] }

        function loop_count_override(loop_selector) {
            logged_calls.push(['loop_count', loop_selector])
        }
        function loop_transition_override(loop_selector, mode, cycles_delay) {
            logged_calls.push(['loop_transition', loop_selector, mode, cycles_delay])
        }

        function loop_get_all_override() {
            return [[0, 0], [1, 1], [2, 2]]
        }
        function loop_get_which_selected_override() {
            return [[0, 0], [2, 2]]
        }
    }

    Interface {
        id: itf
    }

    configuration: MidiControlConfiguration {
        contents: [
            { 
                'filters': [MidiControl.match_type(Midi.NoteOn), MidiControl.match_note(1)],
                'action': 'shoop_control.loop_count({{0, 0}})'
            },
            {
                'filters': [MidiControl.match_type(Midi.NoteOn), MidiControl.match_note(2)],
                'action': 'Stop All'
            },
            {
                'filters': [MidiControl.match_type(Midi.NoteOn), MidiControl.match_note(3)],
                'action': 'Stop All',
                'inputs': {
                    'loops': 'selection'
                }
            },
            {
                'filters': [MidiControl.match_type(Midi.NoteOn), MidiControl.match_note(4)],
                'action': 'Stop All',
                'inputs': {
                    'loops': '{{1, 1}}'
                }
            },
            {
                'filters': [MidiControl.match_type(Midi.NoteOn), MidiControl.match_note(5)],
                'action': 'Stop All',
                'condition': 'false'
            },
            {
                'filters': [MidiControl.match_type(Midi.NoteOn), MidiControl.match_note(6)],
                'action': 'Stop All',
                'condition': 'true'
            },
        ]
    }

    ShoopTestCase {
        name: 'MidiControl'
        filename : TestFilename.test_filename()
        when: ctl.ready

        function test_midi_control_custom_action() {
            run_case('test_midi_control_custom_action', () => {
                itf.clear()
                ctl.handle_midi(Midi.create_noteOn(0, 1, 127))

                verify_eq(itf.logged_calls, [
                    [ 'loop_count', [[0, 0]] ]
                ])
            })
        }

        function test_midi_control_default_action() {
            run_case('test_midi_control_default_action', () => {
                itf.clear()
                ctl.handle_midi(Midi.create_noteOn(0, 2, 127))

                verify_eq(itf.logged_calls, [
                    [ 'loop_transition', [[0, 0], [1, 1], [2, 2]], Types.LoopMode.Stopped, 0 ]
                ])
            })
        }

        function test_midi_control_action_with_input_preset() {
            run_case('test_midi_control_action_with_input_preset', () => {
                itf.clear()
                ctl.handle_midi(Midi.create_noteOn(0, 3, 127))

                verify_eq(itf.logged_calls, [
                    [ 'loop_transition', [[0, 0], [2, 2]], Types.LoopMode.Stopped, 0 ]
                ])
            })
        }

        function test_midi_control_action_with_custom_input() {
            run_case('test_midi_control_action_with_custom_input', () => {
                itf.clear()
                ctl.handle_midi(Midi.create_noteOn(0, 4, 127))

                verify_eq(itf.logged_calls, [
                    [ 'loop_transition', [[1, 1]], Types.LoopMode.Stopped, 0 ]
                ])
            })
        }

        function test_midi_control_action_with_condition_false() {
            run_case('test_midi_control_action_with_condition_false', () => {
                itf.clear()
                ctl.handle_midi(Midi.create_noteOn(0, 5, 127))

                verify_eq(itf.logged_calls, [])
            })
        }

        function test_midi_control_action_with_condition_true() {
            run_case('test_midi_control_action_with_condition_true', () => {
                itf.clear()
                ctl.handle_midi(Midi.create_noteOn(0, 6, 127))

                verify_eq(itf.logged_calls, [
                    [ 'loop_transition', [[0, 0], [1, 1], [2, 2]], Types.LoopMode.Stopped, 0 ]
                ])
            })
        }
    }
}