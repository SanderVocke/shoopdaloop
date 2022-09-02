def channel(msg):
    return msg['data'][0] & 0x0F

def note(msg):
    return msg['data'][1]

def velocity(msg):
    return msg['data'][2]

def is_noteOn(msg):
    return (msg['data'][0] & 0xF0) == 0x90

def is_noteOff(msg):
    return (msg['data'][0] & 0xF0) == 0x80

def is_cc(msg):
    return (msg['data'][0] & 0xF0) == 0xB0

def is_all_notes_off(msg):
    return is_cc(msg) and msg['data'][1] == 123

def is_all_sound_off(msg):
    return is_cc(msg) and msg['data'][1] == 120

def msgs_to_notes(msgs):
    active_note_times = [None for i in range(128 * 16)] # Track all notes on all channels
    notes = []

    def note_idx(msg):
        return channel(msg) * 128 + note(msg)

    def is_note_active(msg):
        return active_note_times[note_idx(msg)] != None

    def terminate_note(start, end, channel, note):
        _note = {
            'start': start,
            'end': end,
            'channel': channel,
            'note': note
        }
        notes.append(_note)
        active_note_times[channel*128+note] = None
    
    def terminate_note_by_msg(msg):
        terminate_note(
            active_note_times[note_idx(msg)],
            msg['time'],
            channel(msg),
            note(msg)
        )
    
    def terminate_channel_notes(channel, time):
        for note in range(128):
            if active_note_times[channel*128 + note] != None:
                terminate_note(
                    active_note_times[i],
                    time,
                    channel,
                    note
                )

    for msg in msgs:
        if is_noteOn(msg) and not is_note_active(msg):
            active_note_times[note_idx(msg)] = msg['time']
        elif is_noteOff(msg) and is_note_active(msg):
            terminate_note_by_msg(msg)
        elif is_all_notes_off(msg):
            terminate_channel_notes(channel(msg), msg['time'])
        elif is_all_sound_off(msg):
            terminate_channel_notes(channel(msg), msg['time'])
    
    #print("Parsed {} notes.".format(len(notes)))
    
    return notes
        