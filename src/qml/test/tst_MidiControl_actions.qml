import QtQuick 6.6
import ShoopDaLoop.Rust
import '../js/midi.js' as Midi
import '../js/midi_control.js' as MidiControl
import './testfilename.js' as TestFilename
import '..'

ShoopTestFile {
    Item {
        component FakeLoop: Item {
            property int track_idx
            property int idx_in_track
            function transition(a,b,c,d) {}
        }

        FakeLoop {
            id: fakeloop1
            track_idx: 0
            idx_in_track: 0
        }
        FakeLoop {
            id: fakeloop2
            track_idx: 1
            idx_in_track: 1
        }
        FakeLoop {
            id: fakeloop3
            track_idx: 2
            idx_in_track: 2
        }

        SessionControlHandler {
            id: itf
            Component.onCompleted: {
                itf.set_test_logging_enabled(true)
            }
            loop_widget_references: [fakeloop1, fakeloop2, fakeloop3]
            selected_loops: [fakeloop1, fakeloop3]
            targeted_loop: undefined
        }

        MidiControl {
            id: ctl
            control_interface: itf

            configuration: MidiControlConfiguration {
                id: config
                contents: [
                    {
                        'filters': [MidiControl.match_type(Midi.NoteOn), MidiControl.match_note(1)],
                        'action': 'shoop_control.loop_count({{0, 0}})'
                    },
                    {
                        'filters': [MidiControl.match_type(Midi.NoteOn), MidiControl.match_note(2)],
                        'action': 'Loop Transition',
                        'inputs': {
                            'loops': 'all'
                        },
                    },
                    {
                        'filters': [MidiControl.match_type(Midi.NoteOn), MidiControl.match_note(3)],
                        'action': 'Loop Transition',
                        'inputs': {
                            'loops': 'selection'
                        }
                    },
                    {
                        'filters': [MidiControl.match_type(Midi.NoteOn), MidiControl.match_note(4)],
                        'action': 'Loop Transition',
                        'inputs': {
                            'loops': '{{1, 1}}'
                        }
                    },
                    {
                        'filters': [MidiControl.match_type(Midi.NoteOn), MidiControl.match_note(5)],
                        'action': 'Loop Transition',
                        'condition': 'false'
                    },
                    {
                        'filters': [MidiControl.match_type(Midi.NoteOn), MidiControl.match_note(6)],
                        'action': 'Loop Transition',
                        'inputs': {
                            'loops': 'all'
                        },
                        'condition': 'true'
                    },
                ]
            }
        }

        ShoopTestCase {
            name: 'MidiControl_actions'
            filename : TestFilename.test_filename()
            when: ctl.ready

            test_fns: ({
                'test_validate_config': () => {
                    verify_eq(config.validate(), true)
                },

                'test_midi_control_custom_action': () => {
                    itf.clear_logged_calls()
                    ctl.handle_midi(Midi.create_noteOn(0, 1, 127), null)

                    verify_eq(itf.logged_calls(), ["loop_count_[[0, 0]]"])
                },

                'test_midi_control_default_action': () => {
                    itf.clear_logged_calls()
                    ctl.handle_midi(Midi.create_noteOn(0, 2, 127), null)

                    verify_eq(itf.logged_calls(), ["loop_transition_[[0, 0], [1, 1], [2, 2]]_mode1_d0_s-1"])
                },

                'test_midi_control_action_with_input_preset': () => {
                    itf.clear_logged_calls()
                    ctl.handle_midi(Midi.create_noteOn(0, 3, 127), null)

                    verify_eq(itf.logged_calls(), ["loop_transition_[[0, 0], [2, 2]]_mode1_d0_s-1"])
                },

                'test_midi_control_action_with_custom_input': () => {
                    itf.clear_logged_calls()
                    ctl.handle_midi(Midi.create_noteOn(0, 4, 127), null)

                    verify_eq(itf.logged_calls(), ["loop_transition_[[1, 1]]_mode1_d0_s-1"])
                },

                'test_midi_control_action_with_condition_false': () => {
                    itf.clear_logged_calls()
                    ctl.handle_midi(Midi.create_noteOn(0, 5, 127), null)

                    verify_eq(itf.logged_calls(), [])
                },

                'test_midi_control_action_with_condition_true': () => {
                    itf.clear_logged_calls()
                    ctl.handle_midi(Midi.create_noteOn(0, 6, 127), null)

                    verify_eq(itf.logged_calls(), ["loop_transition_[[0, 0], [1, 1], [2, 2]]_mode1_d0_s-1"])
                }
            })
        }
    }
}