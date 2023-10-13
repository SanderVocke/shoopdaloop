function loops_input(_default='selection') {
    return {
        'description': 'The loop(s) to control',
        'presets': {
            'selection': 'shoop_control.loop_get_which_selected()',
            'all': 'shoop_control.loop_get_all()',
        },
        'default': _default,
    }
}

// Specifies built-in actions that can be mapped. An action is a named script command which can also take inputs.
// Inputs have default values and presets.
const builtin_actions = ({
    'Stop All': {
        'description': 'Stop all loops',
        'script': 'shoop_control.loop_transition(loops, shoop_control.constants.LoopMode_Stopped, 0)',
        'inputs': {
            'loops': loops_input('all')
        },
    },
    'Default Loop Action': {
        'description': 'Perform default action on (a) loop(s). The default action is designed to cycle through the recording, stopped and playback modes in an intuitive way.',
        'script': 'shoop_helpers.default_loop_action(loops)',
        'inputs': {
            'loops': loops_input('selection')
        },
    },
    'Right': {
        'description': 'Move selection right',
        'script': 'shoop_helpers.move_selection(shoop_control.constants.Key_Right)',
        'inputs': {},
    },
    'Left': {
        'description': 'Move selection left',
        'script': 'shoop_helpers.move_selection(shoop_control.constants.Key_Left)',
        'inputs': {},
    },
    'Up': {
        'description': 'Move selection up',
        'script': 'shoop_helpers.move_selection(shoop_control.constants.Key_Up)',
        'inputs': {},
    },
    'Down': {
        'description': 'Move selection down',
        'script': 'shoop_helpers.move_selection(shoop_control.constants.Key_Down)',
        'inputs': {},
    },
})

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