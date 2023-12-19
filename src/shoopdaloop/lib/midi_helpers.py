def channel(msg_data):
    return msg_data[0] & 0x0F

def note(msg_data):
    return msg_data[1]

def velocity(msg_data):
    return msg_data[2]

def is_noteOn(msg_data):
    return (msg_data[0] & 0xF0) == 0x90

def noteOn(channel, note, velocity):
    return [0x90 + channel, note, velocity]

def is_noteOff(msg_data):
    return (msg_data[0] & 0xF0) == 0x80

def noteOff(channel, note, velocity):
    return [0x80 + channel, note, velocity]

def is_cc(msg_data):
    return (msg_data[0] & 0xF0) == 0xB0

def is_all_notes_off(msg_data):
    return is_cc(msg_data) and msg_data[1] == 123

def is_all_sound_off(msg_data):
    return is_cc(msg_data) and msg_data[1] == 120

def msgs_to_notes(msgs):
    active_note_times = [None for i in range(128 * 16)] # Track all notes on all channels
    notes = []

    def note_idx(msg_data):
        return channel(msg_data) * 128 + note(msg_data)

    def is_note_active(msg_data):
        return active_note_times[note_idx(msg_data)] != None

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
            active_note_times[note_idx(msg['data'])],
            msg['time'],
            channel(msg['data']),
            note(msg['data'])
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
        if is_noteOn(msg['data']) and not is_note_active(msg['data']):
            active_note_times[note_idx(msg['data'])] = msg['time']
        elif is_noteOff(msg['data']) and is_note_active(msg['data']):
            terminate_note_by_msg(msg)
        elif is_all_notes_off(msg['data']):
            terminate_channel_notes(channel(msg['data']), msg['time'])
        elif is_all_sound_off(msg['data']):
            terminate_channel_notes(channel(msg['data']), msg['time'])
    
    #print("Parsed {} notes.".format(len(notes)))
    
    return notes
        