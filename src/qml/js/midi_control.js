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

function delay_input(_default='0') {
    return {
      'description': 'Delay by # of sync loop cycles (-1 is instant)',
      'default': _default,
    }
}

function align_input(_default='-1') {
  return {
    'description': 'Immediately force align to the sync loop at cycle # (-1 is disable)',
    'default': _default,
  }
}

function mode_input(_default='stopped') {
  return {
    'description': 'Specify loop mode',
    'presets': {
      'stopped': 'shoop_control.constants.LoopMode_Stopped',
      'recording': 'shoop_control.constants.LoopMode_Recording',
      'playing': 'shoop_control.constants.LoopMode_Playing',
      'play w/ FX': 'shoop_control.constants.LoopMode_PlayingDryThroughWet',
      're-record FX': 'shoop_control.constants.LoopMode_RecordingDryIntoWet',
    },
    'default': _default
  }
}

function direction_input(_default='right') {
  return {
      'description': 'Direction',
      'presets': {
          'right': 'shoop_control.constants.Key_Right',
          'left': 'shoop_control.constants.Key_Left',
          'up': 'shoop_control.constants.Key_Up',
          'down': 'shoop_control.constants.Key_Down',
      },
      'default': 'right',
  }
}

function track_input(_default='0') {
  return {
    'description': 'Track',
    'default': _default,
  }
}

function gain_conversion_input(_default='cc/vel') {
  return {
    'description': 'gain level conversion',
    'presets': {
      'cc/vel': 'msg.bytes[2] / 127.0',
    },
    'default': _default,
  }
}

// Specifies built-in actions that can be mapped. An action is a named script command which can also take inputs.
// Inputs have default values and presets.
const builtin_actions = ({
    'Default Loop Action': {
        'description': 'Perform default action on (a) loop(s). The default action is designed to cycle through the recording, stopped and playback modes in an intuitive way.',
        'script': 'shoop_helpers.default_loop_action(loops)',
        'inputs': {
            'loops': loops_input('selection')
        },
    },
    'Loop Transition': {
        'description': 'Transition loop(s) to a mode',
        'script': 'shoop_control.loop_transition(loops, mode, maybe_delay_cycles, maybe_align_with_sync_cycle)',
        'inputs': {
          'loops': loops_input('selection'),
          'mode': mode_input('stopped'),
          'maybe_delay_cycles': delay_input('0'),
          'maybe_align_with_sync_cycle': align_input('-1')
        }
    },
    'Move Selection': {
        'description': 'Move selection in a direction',
        'script': 'shoop_helpers.move_selection(direction)',
        'inputs': {
            'direction': direction_input(),
        },
    },
    'Set Track gain': {
        'description': 'Set gain level of (a) track(s)',
        'script': 'shoop_control.track_set_gain_fader(track, value)',
        'inputs': {
          'track': track_input('0'),
          'value': gain_conversion_input('cc/vel')
        }
    }
})

const default_action_config =({
  'action': 'Default Loop Action',
  'inputs': {
    'loops': 'selection'
  },
  'condition': null
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
function match_cc(cc) {
    return [ 1, 0xFF, cc ]
}
function match_channel(channel) {
    return [ 0, 0x0F, channel ]
}
function match_program(program) {
    return [ 1, 0xFF, program ]
}

const MessageFilterKind = {
    NoteOn: 'Note on',
    NoteOff: 'Note off',
    ControlChange: 'CC',
    ProgramChange: 'Program change',
    Advanced: 'Advanced'
  }

  function parse_midi_filters(filters) {
  
    let msgtype_filters = filters.filter(f => (f[0] === 0 && is_msgtype_mask(f[1])))
    let channel_filters = filters.filter(f => f && f[0] === 0 && f[1] === 0x0F)
    let _2ndbyte_filters = filters.filter(f => (f[0] === 1))
    let _3rdbyte_filters = filters.filter(f => (f[0] === 2))
  
    var kind = null

    if (
      msgtype_filters.length === 1
      && channel_filters.length <= 1
      && msgtype_filters[0][2] === Midi.NoteOn) {
      kind = MessageFilterKind.NoteOn
    } else if (msgtype_filters.length === 1
              && channel_filters.length <= 1
              && msgtype_filters[0][2] === Midi.NoteOff) {
      kind = MessageFilterKind.NoteOff
    } else if (msgtype_filters.length === 1
               && channel_filters.length <= 1
               && msgtype_filters[0][2] === Midi.ControlChange) {
      kind = MessageFilterKind.ControlChange
    } else if (msgtype_filters.length === 1
               && channel_filters.length <= 1
               && msgtype_filters[0][2] === Midi.ProgramChange) {
      kind = MessageFilterKind.ProgramChange
    }
  
    if (kind === null) {
      kind = MessageFilterKind.Advanced
    }
  
    var rval = {
      'kind': kind,
    }

    if (supports_note(rval) && _2ndbyte_filters.length === 1 &&
        is_identity_mask(_2ndbyte_filters[0][1])) {
      rval.note = _2ndbyte_filters[0][2]
    }
    if (supports_channel(rval) && channel_filters.length === 1) {
      rval.channel = channel_filters[0][2]
    }
    if (supports_program(rval) && _2ndbyte_filters.length === 1 &&
        is_identity_mask(_2ndbyte_filters[0][1])) {
      rval.program = _2ndbyte_filters[0][2]
    }
    if (supports_cc(rval) && _2ndbyte_filters.length === 1 &&
        is_identity_mask(_2ndbyte_filters[0][1])) {
      rval.cc = _2ndbyte_filters[0][2]
    }

    switch (kind) {
      case MessageFilterKind.NoteOn:
        rval.description = (rval.note !== undefined) ? `Note ${rval.note} on` : 'Any note on'
        break;
      case MessageFilterKind.NoteOff:
        rval.description = (rval.note !== undefined) ? `Note ${rval.note} off` : 'Any note off'
        break;
      case MessageFilterKind.ControlChange:
        rval.description = (rval.cc !== undefined) ? `CC ${rval.cc}` : 'Any CC'
        break;
      case MessageFilterKind.ProgramChange:
        rval.description = (rval.program !== undefined) ? `Program Change ${rval.program}` : 'Any Program Change'
        break;
      default:
        rval.description = 'Advanced'
        break;
    }

    if (supports_channel(rval) && has_channel(rval)) {
      rval.description += ` @ ch. ${rval.channel}`
    }

    return rval
  }

  function supports_channel(filters_descriptor) {
    return filters_descriptor.kind === MessageFilterKind.NoteOn ||
    filters_descriptor.kind === MessageFilterKind.NoteOff ||
    filters_descriptor.kind === MessageFilterKind.ControlChange ||
    filters_descriptor.kind === MessageFilterKind.ProgramChange
  }

  function supports_note(filters_descriptor) {
    return filters_descriptor.kind === MessageFilterKind.NoteOn ||
    filters_descriptor.kind === MessageFilterKind.NoteOff
  }

  function supports_program(filters_descriptor) {
    return filters_descriptor.kind === MessageFilterKind.ProgramChange
  }

  function supports_cc(filters_descriptor) {
    return filters_descriptor.kind === MessageFilterKind.ControlChange
  }

  function has_note(filters_descriptor) {
    return Object.keys(filters_descriptor).includes('note') &&
      filters_descriptor.note !== undefined
  }
  
  function has_channel(filters_descriptor) {
    return Object.keys(filters_descriptor).includes('channel') &&
      filters_descriptor.channel !== undefined
  }

  function has_program(filters_descriptor) {
    return Object.keys(filters_descriptor).includes('program') &&
      filters_descriptor.program !== undefined
  }

  function has_cc(filters_descriptor) {
    return Object.keys(filters_descriptor).includes('cc') &&
      filters_descriptor.cc !== undefined
  }

  function is_identity_mask(value) {
    return value & 0x7F === 0x7F
  }

  function is_channel_mask(value) {
    return value === 0x0F
  }

  function is_msgtype_mask(value) {
    return value === 0xF0
  }