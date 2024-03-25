# Functions for IO on an internal, sample-accurate MIDI format.
# The format is just JSON with each message containing data and a timestamp in seconds.
# MIDI port state at the time of recording start is included as MIDI message bytes (no time) in the "start_state" property.
import copy

def generate_smf(messages, total_length, sample_rate):
    msgs = copy.deepcopy(messages)
    for m in msgs:
        m['time'] = float(m['time']) / float(sample_rate)
    state_msgs = [m['data'] for m in msgs if m['time'] < 0.0]
    recorded_msgs = [m for m in msgs if m['time'] >= 0.0]
    length = float(total_length) / float(sample_rate)
    return {
        'messages': recorded_msgs,
        'start_state': state_msgs,
        'length': length
    }

def parse_smf(smf_data, target_sample_rate):
    backend_msgs = smf_data['messages']
    state_msg_datas = smf_data['start_state']
    for msg in backend_msgs:
        msg['time'] = round(msg['time'] * target_sample_rate)
    total_sample_time = round(smf_data['length'] * target_sample_rate)
    return (backend_msgs, state_msg_datas, total_sample_time)