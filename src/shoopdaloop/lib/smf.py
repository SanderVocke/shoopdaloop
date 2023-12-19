# Functions for IO on an internal, sample-accurate MIDI format.
# The format is just JSON with each message containing data and a timestamp in seconds.
import copy

def generate_smf(messages, total_length, sample_rate):
    msgs = copy.deepcopy(messages)
    for m in msgs:
        m['time'] = float(m['time']) / float(sample_rate)
    length = float(total_length) / float(sample_rate)
    return {
        'messages': msgs,
        'length': length
    }

def parse_smf(smf_data, target_sample_rate):
    backend_msgs = smf_data['messages']
    for msg in backend_msgs:
        msg['time'] = round(msg['time'] * target_sample_rate)
    total_sample_time = round(smf_data['length'] * target_sample_rate)
    return (backend_msgs, total_sample_time)