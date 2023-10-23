local shoop_midi = {}

shoop_midi.NoteOn = 0x90
shoop_midi.NoteOff = 0x80
shoop_midi.ControlChange = 0xB0
shoop_midi.ProgramChange = 0xC0
shoop_midi.PitchBend = 0xE0
shoop_midi.Aftertouch = 0xA0
shoop_midi.PolyAftertouch = 0xD0
shoop_midi.SysEx = 0xF0

function shoop_midi.is_kind(msg, kind)
    return (msg.bytes[0] & 0XF0) == kind
end

return shoop_midi