/* 
input data layout:
{
    "request_id": 1,
    "samples_per_bin": 16,
    "channels_data": [
        ["channel_1_id", {
            "audio": [0, 1, 2, 3, ...]
            "midi_notes": [
                { start: 0, end: 10, note: 100, channel: 4 },
                ...
            ],
            "start_offset": 256
        }],
        ...
    ]
}

where:
- samples_per_bin is the desired samples per bin
- the audio data arrays are the raw data arrays as stored in the channels (lengths may differ)
- the start offsets are in samples, not bins

output data (callback message) layout:
{
    "request_id": 1,
    "samples_per_bin": 16,
    "start_offset": 256,
    "channels_data": [
        ["channel_1_id", {
            "audio": {
                "transient_positive": [0, 1, 2, 3, ...]
                "transient_negative": [0, 1, 2, 3, ...]
                "rms": [0, 1, 2, 3, ...]
            },
            "midi_notes": [
                { start: 0, end: 10, note: 100, channel: 4 },
                ...
            ],
            "pre_padding": 128
        }],
        ...
    ]
}

where:
- samples_per_bin is what was requested originally
- the audio data arrays are all equal in length and padded such that their start positions align
- the start_offset in samples is the same for each array because of the padding.
- pre_padding per channel indicates how many samples of padding were added for that channel.
*/
WorkerScript.onMessage = function(input_data) {
    var samples_per_bin = input_data['samples_per_bin']
    var rval = {}

    // A "pos" refers to the index of a sample w.r.t. the loop start
    var first_sample_pos = 0
    var last_sample_pos = 0
    var chan_first_sample_pos, chan_last_sample_pos
    var chan_last_audio_sample_pos, chan_last_midi_sample_pos
    for (var channel_data of input_data['channels_data']) {
        chan_first_sample_pos = -channel_data[1]['start_offset']
        chan_last_audio_sample_pos = ('audio' in channel_data[1]) ? chan_first_sample_pos + channel_data[1]['audio'].length : 0
        chan_last_midi_sample_pos = ('midi_notes' in channel_data[1] && channel_data[1]['midi_notes'].length > 0) ? 
            chan_first_sample_pos + channel_data[1]['midi_notes'][channel_data[1]['midi_notes'].length - 1]['end'] : 0
        chan_last_sample_pos = Math.max(chan_last_audio_sample_pos, chan_last_midi_sample_pos)
        first_sample_pos = Math.min(first_sample_pos, chan_first_sample_pos)
        last_sample_pos = Math.max(last_sample_pos, chan_last_sample_pos)
    }
    var n_samples = last_sample_pos - first_sample_pos;
    var n_bins = Math.ceil(n_samples / samples_per_bin)

    var output_datas = []
    var id, chan_start_offset, chan_audio_data, chan_midi_notes, transients_positive, transients_negative, rmss
    var transient_positive, transient_negative, rms, sample
    var pre_padding, padded_midi_notes
    for (var channel_data of input_data['channels_data']) {
        id = channel_data[0]
        chan_start_offset = channel_data[1]['start_offset']
        chan_audio_data = ('audio' in channel_data[1]) ? channel_data[1]['audio'] : undefined
        chan_midi_notes = ('midi_notes' in channel_data[1]) ? channel_data[1]['midi_notes'] : undefined
        chan_first_sample_pos = -channel_data[1]['start_offset']
        chan_last_audio_sample_pos = ('audio' in channel_data[1]) ? chan_first_sample_pos + channel_data[1]['audio'].length : 0
        chan_last_midi_sample_pos = (chan_midi_notes && chan_midi_notes.length > 0) ? 
            chan_first_sample_pos + chan_midi_notes[chan_midi_notes.length - 1]['end'] : 0
        chan_last_sample_pos = Math.max(chan_last_audio_sample_pos, chan_last_midi_sample_pos)
        if (chan_audio_data) {
            transients_positive = Array(n_bins)
            transients_negative = Array(n_bins)
            rmss = Array(n_bins)
            for (var bin_idx = 0; bin_idx < n_bins; bin_idx++) {
                transient_negative = 0.0
                transient_positive = 0.0
                rms = 0.0
                for (var sample_pos = first_sample_pos + (bin_idx * samples_per_bin);
                    sample_pos < first_sample_pos + (((bin_idx+1) * samples_per_bin));
                    sample_pos++)
                {
                    sample = (sample_pos >= chan_first_sample_pos && sample_pos < chan_last_sample_pos) ?
                        chan_audio_data[sample_pos - chan_first_sample_pos] : 0.0
                    if (sample > 0.0) { transient_positive = Math.max(transient_positive, sample) }
                    else { transient_negative = Math.min(transient_negative, sample) }
                    rms += Math.sqrt(sample * sample)
                }
                rms /= samples_per_bin
                transients_positive[bin_idx] = transient_positive
                transients_negative[bin_idx] = transient_negative
                rmss[bin_idx] = rms
            }
        }
        pre_padding = first_sample_pos + chan_start_offset
        padded_midi_notes = chan_midi_notes ?
            chan_midi_notes.map(n => {
                var m = n
                m['start'] += pre_padding
                m['end'] += pre_padding
                return m}) : undefined
        
        var d = {
            'pre_padding': pre_padding
        }
        if (chan_audio_data) {
            d['audio'] = {
                "transient_positive": transients_positive,
                "transient_negative": transients_negative,
                "rms": rmss
            }
        }
        if (padded_midi_notes) {
            d['midi_notes'] = padded_midi_notes
        }

        output_datas.push([
            id, d
        ])
    }

    // Construct return value
    rval = {
        'request_id': input_data['request_id'],
        'samples_per_bin': samples_per_bin,
        'start_offset': -first_sample_pos,
        'channels_data': output_datas
    }
    
    WorkerScript.sendMessage(rval)
}