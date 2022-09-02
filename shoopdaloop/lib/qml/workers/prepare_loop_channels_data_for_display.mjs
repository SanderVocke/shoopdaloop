/* 
input data layout:
{
    "request_id": 1,
    "timesteps_per_bin": 16,
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
- timesteps_per_bin is the desired samples per bin
- the audio data arrays are the raw data arrays as stored in the channels (lengths may differ)
- the start offsets are in samples, not bins

output data (callback message) layout:
{
    "request_id": 1,
    "timesteps_per_bin": 16,
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
- timesteps_per_bin is what was requested originally
- the audio data arrays are all equal in length and padded such that their start positions align
- the start_offset in samples is the same for each array because of the padding.
- pre_padding per channel indicates how many samples of padding were added for that channel.
*/
WorkerScript.onMessage = function(input_data) {
    console.time("update display data")
    var timesteps_per_bin = input_data['samples_per_bin']
    var rval = {}

    // A "pos" / "position" in this function refers to the index of a sample w.r.t. the loop start

    // Add some space to the beginning of the loop, just so that the user can e.g. shift the start offset forward.
    // Note: 50 pixels worth of space.
    const extra_pre_padding = 50*timesteps_per_bin

    const chan_data_start_positions = 
        input_data['channels_data'].map(chan_data => -chan_data[1]['start_offset'])
    const chan_data_end_positions =
        input_data['channels_data'].map((chan_data, idx) => {
            if ('audio' in chan_data[1]) { return chan_data_start_positions[idx] + chan_data[1]['audio'].length }
            else if ('midi_notes' in chan_data[1]) {
                const l = chan_data[1]['midi_notes'].length
                return (l > 0) ?
                    chan_data_start_positions[idx] + chan_data[1]['midi_notes'][l-1]['end'] :
                    0;
            }
            else { return 0; }
        })
    

    const render_start_pos = Math.min.apply(null, chan_data_start_positions) - extra_pre_padding
    const render_end_pos = Math.max.apply(null, chan_data_end_positions)

    const render_n_timesteps = render_end_pos - render_start_pos;
    var render_n_bins = Math.ceil(render_n_timesteps / timesteps_per_bin)

    var output_datas = []
    input_data['channels_data'].forEach((channel_data, channel_idx) => {
        const chan_id = channel_data[0]
        const chan_start_offset = channel_data[1]['start_offset']
        const chan_audio_data = ('audio' in channel_data[1]) ? channel_data[1]['audio'] : undefined
        const chan_midi_notes = ('midi_notes' in channel_data[1]) ? channel_data[1]['midi_notes'] : undefined
        const chan_data_start_pos = chan_data_start_positions[channel_idx]
        const chan_data_end_pos = chan_data_end_positions[channel_idx]

        var transients_negative = []
        var transients_positive = []
        var rmss = []
        if (chan_audio_data) {
            transients_positive = Array(render_n_bins)
            transients_negative = Array(render_n_bins)
            rmss = Array(render_n_bins)
            for (var bin_idx = 0; bin_idx < render_n_bins; bin_idx++) {
                var bin_transient_negative = 0.0
                var bin_transient_positive = 0.0
                var bin_rms = 0.0
                for (var render_pos = render_start_pos + (bin_idx * timesteps_per_bin);
                    render_pos < render_start_pos + (((bin_idx+1) * timesteps_per_bin));
                    render_pos++)
                {
                    const sample = (render_pos >= chan_data_start_pos && render_pos < chan_data_end_pos) ?
                        chan_audio_data[render_pos - chan_data_start_pos] : 0.0
                    if (sample > 0.0) { bin_transient_positive = Math.max(bin_transient_positive, sample) }
                    else { bin_transient_negative = Math.min(bin_transient_negative, sample) }
                    bin_rms += Math.sqrt(sample * sample)
                }
                bin_rms /= timesteps_per_bin
                transients_positive[bin_idx] = bin_transient_positive
                transients_negative[bin_idx] = bin_transient_negative
                rmss[bin_idx] = bin_rms
            }
        }
        const chan_pre_padding = chan_data_start_pos - render_start_pos

        // Re-scale and offset the MIDI notes.
        var rescaled_midi_notes = chan_midi_notes ? chan_midi_notes.map(n => {
            var r = n
            r['start'] = (r['start'] + chan_pre_padding) / timesteps_per_bin
            r['end'] = (r['end'] + chan_pre_padding) / timesteps_per_bin
            return r
        }) : undefined
        
        var d = {
            'included_pre_padding': chan_pre_padding
        }
        if (chan_audio_data) {
            d['audio'] = {
                "transient_positive": transients_positive,
                "transient_negative": transients_negative,
                "rms": rmss
            }
        }
        if (rescaled_midi_notes) {
            d['midi_notes'] = rescaled_midi_notes
        }

        output_datas.push([
            chan_id, d
        ])
    })

    // Construct return value
    rval = {
        'request_id': input_data['request_id'],
        'samples_per_bin': timesteps_per_bin,
        'render_start_pos': render_start_pos,
        'channels_data': output_datas
    }
    
    console.timeEnd("update display data")
    WorkerScript.sendMessage(rval)
}