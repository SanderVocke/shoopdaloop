const NoteOn = 0x90
const NoteOff = 0x80
const ControlChange = 0xB0
const ProgramChange = 0xC0
const PitchBend = 0xE0
const Aftertouch = 0xA0
const PolyAftertouch = 0xD0
const SysEx = 0xF0

function create_noteOn(channel, note, velocity) {
  return [NoteOn + channel, note, velocity]
}

function maybe_note(msg) {
  if ([NoteOn, NoteOff].includes(msg[0] & 0xF0)) {
    return msg[1]
  }
  return null
}

function maybe_cc(msg) {
  if ((msg[0] & 0xF0) === ControlChange) {
    return msg[1]
  }
  return null
}

function maybe_program(msg) {
  if ((msg[0] & 0xF0) === ProgramChange) {
    return msg[1]
  }
  return null
}

function channel(msg) {
  return msg[0] & 0x0F
}