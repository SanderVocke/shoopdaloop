import QtQuick 6.6

import ShoopConstants
import '../../midi.js' as Midi
import '../../midi_control.js' as MidiControl
import './testfilename.js' as TestFilename
import '..'

ShoopTestFile {
    Item {
        // A dummy control interface which logs calls for testing
        component Interface: LuaControlInterface {
            qml_instance: this
            property var logged_calls: new Set()
            function clear() { logged_calls = new Set() }

            function loop_count_override(loop_selector) {
                logged_calls.add(loop_selector)
            }
        }

        Interface {
            id: itf

            MidiControl {
                id: ctl
                control_interface: itf

                configuration: MidiControlConfiguration {
                    contents: [
                        { 
                            'filters': [],
                            'action': 'shoop_control.loop_count({{0, 0}})'
                        },
                        { 
                            'filters': [MidiControl.match_type(Midi.NoteOn)],
                            'action': 'shoop_control.loop_count({{1, 1}})'
                        },
                        { 
                            'filters': [MidiControl.match_type(Midi.NoteOff)],
                            'action': 'shoop_control.loop_count({{2, 2}})'
                        },
                        { 
                            'filters': [MidiControl.match_type(Midi.ControlChange)],
                            'action': 'shoop_control.loop_count({{3, 3}})'
                        },
                        { 
                            'filters': [MidiControl.match_type(Midi.ProgramChange)],
                            'action': 'shoop_control.loop_count({{4, 4}})'
                        },
                    ]
                }
            }
        }

        ShoopTestCase {
            name: 'MidiControl_filters'
            filename : TestFilename.test_filename()
            when: ctl.ready

            test_fns: ({
                'test_midi_control_filter_noteon': () => {
                    itf.clear()
                    ctl.handle_midi([0, 0xF0, 0x90])

                    verify_eq(itf.logged_calls, new Set([ [0, 0], [1, 1] ]))
                },

                'test_midi_control_filter_noteoff': () => {
                    itf.clear()
                    ctl.handle_midi([0, 0xF0, 0x80])

                    verify_eq(itf.logged_calls, new Set([ [0, 0], [2, 2] ]))
                },
            })
        }
    }
}