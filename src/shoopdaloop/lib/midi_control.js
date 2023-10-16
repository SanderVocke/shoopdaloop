.import 'midi.js' as Midi;

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

const MessageFilterKind = {
    AnyNoteOn: 0,
    AnyNoteOff: 1,
    SpecificNoteOn: 2,
    SpecificNoteOff: 3,
    AnyControlChange: 4,
    SpecificControlChange: 5,
    Advanced: 6
  }
  
  const UiMessageFilterKind = {
    NoteOn: 'Note on',
    NoteOff: 'Note off',
    ControlChange: 'CC',
    Advanced: 'Advanced'
  }
  
  const KindToUiKind = {
    [MessageFilterKind.AnyNoteOn]: UiMessageFilterKind.NoteOn,
    [MessageFilterKind.AnyNoteOff]: UiMessageFilterKind.NoteOff,
    [MessageFilterKind.SpecificNoteOn]: UiMessageFilterKind.NoteOn,
    [MessageFilterKind.SpecificNoteOff]: UiMessageFilterKind.NoteOff,
    [MessageFilterKind.AnyControlChange]: UiMessageFilterKind.ControlChange,
    [MessageFilterKind.SpecificControlChange]: UiMessageFilterKind.ControlChange,
    [MessageFilterKind.Advanced]: UiMessageFilterKind.Advanced
  }

  function parse_midi_filters(filters) {
    function select_filter_values(byte, mask) {
      return filters.filter(f => f && f.length >= 3 &&f[0] === byte && f[1] === mask).map(f => f[2])
    }
  
    let msgtype_filters = select_filter_values(0, 0xF0)
    let _1stbyte_filters = filters.filter(f => f && f[0] === 0)
    let _2ndbyte_filters = select_filter_values(1, 0xFF)
    let _3rdbyte_filters = select_filter_values(2, 0xFF)
  
    var kind = null
    var description = null
    var note = undefined
    var cc = undefined
  
    if (
      msgtype_filters.length === 1
      && _1stbyte_filters.length === 1
      && msgtype_filters[0] === Midi.NoteOn) {
      if (_2ndbyte_filters.length === 0 && _3rdbyte_filters.length === 0) {
        kind = MessageFilterKind.AnyNoteOn
        description = 'Any note on'
      } else if (_2ndbyte_filters.length === 1 && _3rdbyte_filters.length <= 1) {
        kind = MessageFilterKind.SpecificNoteOn
        note = _2ndbyte_filters[0]
        description = `Note on ${_2ndbyte_filters[0]}` +
          (_3rdbyte_filters.length == 1 ? ` on ch. ${_3rdbyte_filters[0]}` : '')
      }
    } else if (msgtype_filters.length === 1
              && _1stbyte_filters.length === 1
              && msgtype_filters[0] === Midi.NoteOff) {
      if (_2ndbyte_filters.length === 0 && _3rdbyte_filters.length === 0) {
        kind = MessageFilterKind.AnyNoteOff
        description = 'Any note off'
      } else if (_2ndbyte_filters.length === 1 && _3rdbyte_filters.length <= 1) {
        kind = MessageFilterKind.SpecificNoteOff
        note = _2ndbyte_filters[0]
        description = `Note off ${_2ndbyte_filters[0]}` +
          (_3rdbyte_filters.length == 1 ? ` on ch. ${_3rdbyte_filters[0]}` : '')
      }
    } else if (msgtype_filters.length === 1
               && _1stbyte_filters.length === 1
               && msgtype_filters[0] === Midi.ControlChange) {
      if (_2ndbyte_filters.length === 0 && _3rdbyte_filters.length === 0) {
        kind = MessageFilterKind.AnyControlChange
        description = 'Any CC'
      } else if (_2ndbyte_filters.length === 1 && _3rdbyte_filters.length <= 1) {
        kind = MessageFilterKind.SpecificControlChange
        cc = _2ndbyte_filters[0]
        description = `CC ${_2ndbyte_filters[0]}` +
          (_3rdbyte_filters.length == 1 ? ` set to ${_3rdbyte_filters[0]}` : '')
      }
    }
  
    if (kind === null) {
      kind = MessageFilterKind.Advanced 
      description = 'Advanced rule'
    }
  
    return {
      'kind': kind,
      'description': description,
      'note': note,
      'cc': cc
    }
  }