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
        FakeLoop {
            id: fakeloop4
            track_idx: 3
            idx_in_track: 3
        }
        FakeLoop {
            id: fakeloop5
            track_idx: 4
            idx_in_track: 4
        }

        SessionControlHandler {
            id: itf
            Component.onCompleted: {
                itf.set_test_logging_enabled(true)
            }
            loop_widget_references: [fakeloop1, fakeloop2, fakeloop3, fakeloop4, fakeloop5]
            selected_loops: []
            targeted_loop: undefined
        }

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

        ShoopTestCase {
            name: 'MidiControl_filters'
            filename : TestFilename.test_filename()
            when: ctl.ready

            test_fns: ({
                'test_midi_control_filter_noteon': () => {
                    itf.clear_logged_calls()
                    ctl.handle_midi([0x90, 127, 127])

                    verify_eq(itf.logged_calls(), ["loop_count_[[0, 0]]","loop_count_[[1, 1]]"])
                },

                'test_midi_control_filter_noteoff': () => {
                    itf.clear_logged_calls()
                    ctl.handle_midi([0x80, 127, 127])

                    verify_eq(itf.logged_calls(), ["loop_count_[[0, 0]]","loop_count_[[2, 2]]"])
                },
            })
        }
    }
}