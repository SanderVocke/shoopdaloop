const NoteOn = 0x90
const NoteOff = 0x80
const ControlChange = 0xB0
const ProgramChange = 0xC0
const PitchBend = 0xE0
const Aftertouch = 0xA0
const PolyAftertouch = 0xD0
const SysEx = 0xF0

const MessageFilterKind = {
  AnyNoteOn: 0,
  AnyNoteOff: 1,
  SpecificNoteOn: 2,
  SpecificNoteOff: 3,
  AnyControlChange: 4,
  SpecificControlChange: 5,
  Advanced: 6
}

function create_noteOn(channel, note, velocity) {
  return [NoteOn + channel, note, velocity]
}

function parse_midi_filters(filters) {
  function select_filter_values(byte, mask) {
    return filters.filter(f => f[0] === byte && f[1] === mask).map(f => f[2])
  }

  let msgtype_filters = select_filter_values(0, 0xF0)
  let _2ndbyte_filters = select_filter_values(1, 0xFF)
  let _3rdbyte_filters = select_filter_values(2, 0xFF)

  var kind = null
  var description = null

  if (msgtype_filters.length === 1 && msgtype_filters[0] === NoteOn) {
    if (_2ndbyte_filters.length === 0 && _3rdbyte_filters.length === 0) {
      kind = MessageFilterKind.AnyNoteOn
      description = 'Any note on'
    } else if (_2ndbyte_filters.length === 1 && _3rdbyte_filters.length <= 1) {
      kind = MessageFilterKind.SpecificNoteOn
      description = `Note on ${_2ndbyte_filters[0]}` +
        (_3rdbyte_filters.length == 1 ? ` on ch. ${_3rdbyte_filters[0]}` : '')
    }
  }

  if (kind === null) {
    kind = MessageFilterKind.Advanced 
    description = 'Advanced rule'
  }

  return {
    'kind': kind,
    'description': description
  }
}