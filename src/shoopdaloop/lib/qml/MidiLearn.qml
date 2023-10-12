import QtQuick 6.3
import ShoopDaLoop.PythonLogger

import '../midi.js' as Midi

Item {
    id: root
    property bool when: false

    readonly property var builtin_actions: ({
        'Stop All': 'shoop_control.loop_transition(shoop_control.loop_get_all(), shoop_control.constants.LoopMode_Stopped, 0)',
        'Right': 'shoop_helpers.move_selection(shoop_control.constants.Key_Right)',
        'Left': 'shoop_helpers.move_selection(shoop_control.constants.Key_Left)',
        'Up': 'shoop_helpers.move_selection(shoop_control.constants.Key_Up)',
        'Down': 'shoop_helpers.move_selection(shoop_control.constants.Key_Down)',
        'APC Loop': 'print_info(msg[1]); if msg[1] < 64 then shoop_helpers.default_loop_action({{0, 0}}) end',
    })
    
    property var action_impls : {}

    function initialize() {
        if (!when) { return; }

        root.logger.debug('Initializing')

        scripting_context = scripting_engine.new_context()
        scripting_engine.execute(`
declare_in_context('shoop_control', require('shoop_control'))
declare_in_context('shoop_coords', require('shoop_coords'))
declare_in_context('shoop_helpers', require('shoop_helpers'))
declare_in_context('shoop_format', require('shoop_format'))
`, scripting_context, 'MidiLearn', true, true)
        let context = scripting_context
        var rval = {}
        for(const name in builtin_actions) {
            let script = `return function(msg) ${builtin_actions[name]} end`
            let fn = scripting_engine.evaluate(script, context, 'MidiLearn', true, true)
            let _name = name
            rval[_name] = function(msg, _fn=fn, name=_name, _context=context) {
                root.logger.debug(`Running action ${name}`)
                scripting_engine.call(_fn, [msg], _context)
            }
        }
        action_impls = rval
    }

    onWhenChanged: initialize()
    Component.onCompleted: initialize()

    property var scripting_context: null

    Component.onDestruction: {
        if (scripting_context !== null) {
            scripting_engine.delete_context(scripting_context)
        }
    }

    // To specify what MIDI messages to react to, and how to react to them,
    // we use filters and input extractors.
    // A filter is a structure as follows:
    // [ byte_nr, mask, compare_value ]
    // e.g. [ 0, 0xF0, 0x90 ] will match on noteOn messages.

    function match_type(type) {
        return [ 0, 0xF0, type ]
    }
    function match_note(note) {
        return [ 1, 0xFF, note ]
    }

    // The configuration is a list of [ [...filters], action ]
    // TODO: some optimization in the implementation so that we don't have to iterate over all the filters
    // for every single message
    property var configuration : [
        [ [match_type(Midi.NoteOn), match_note(89)], 'Stop All'],
        [ [match_type(Midi.NoteOn), match_note(64)], 'Select Move Up'],
        [ [match_type(Midi.NoteOn), match_note(65)], 'Select Move Down'],
        [ [match_type(Midi.NoteOn), match_note(66)], 'Select Move Left'],
        [ [match_type(Midi.NoteOn), match_note(67)], 'Select Move Right'],
        [ [match_type(Midi.NoteOn)], 'APC Loop' ]
    ]

    readonly property PythonLogger logger: PythonLogger { name: "Frontend.Qml.MidiLearn" }

    function handle_midi(msg) {
        let try_filter = function(filter, msg) {
            let byte_data = msg[filter[0]]
            let result = byte_data & filter[1]
            return result == filter[2]
        }

        for(var i=0; i<configuration.length; i++) {
            const filters = configuration[i][0]
            const action = configuration[i][1]
            if (!( new Set(filters.map(f => try_filter(f, msg))).has(false) )) {
                root.logger.debug(`Matched MIDI learn filter: msg = ${msg}, filters = ${filters}, action = ${action}`)
                action_impls[action](msg)
            } else {
                root.logger.trace(`No match for MIDI learn filter: msg = ${msg}, filters = ${filters}, action = ${action}`)
            }
        }
    }
}