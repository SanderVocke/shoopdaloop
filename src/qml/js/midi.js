const NoteOn = 0x90
const NoteOff = 0x80
const ControlChange = 0xB0
const ProgramChange = 0xC0
const PitchBend = 0xE0
const Aftertouch = 0xA0
const PolyAftertouch = 0xD0
const SysEx = 0xF0

const AllSoundOff = 120

function create_noteOn(channel, note, velocity) {
  return [NoteOn + channel, note, velocity]
}

function create_noteOff(channel, note, velocity) {
  return [NoteOff + channel, note, velocity]
}

function create_cc(channel, cc, value) {
  return [ControlChange + channel, cc, value]
}

function create_all_sound_off(channel) {
  return create_cc(channel, AllSoundOff, 0)
}

function maybe_note(msg) {
  if ([NoteOn, NoteOff].includes(msg[0] & 0xF0)) {
    return msg[1]
  }
  return undefined
}

function maybe_cc(msg) {
  if ((msg[0] & 0xF0) === ControlChange) {
    return msg[1]
  }
  return undefined
}

function maybe_program(msg) {
  if ((msg[0] & 0xF0) === ProgramChange) {
    return msg[1]
  }
  return undefined
}

function maybe_velocity(msg) {
  if ([NoteOn, NoteOff].includes(msg[0] & 0xF0)) {
    return msg[2]
  }
  return undefined
}

function maybe_value(msg) {
  if ((msg[0] & 0xF0) === ControlChange) {
    return msg[2]
  }
  return undefined
}

function channel(msg) {
  return msg[0] & 0x0F
}

function parse_msg(msg) {
  return {
      'bytes': msg,
      'note': Midi.maybe_note(msg),
      'channel': Midi.channel(msg),
      'cc': Midi.maybe_cc(msg),
      'program': Midi.maybe_program(msg),
      'velocity': Midi.maybe_velocity(msg),
      'value': Midi.maybe_value(msg)
  }
}