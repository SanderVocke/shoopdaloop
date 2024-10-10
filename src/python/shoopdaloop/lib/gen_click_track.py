import wave
import argparse
from .midi_helpers import noteOn, noteOff
from .smf import generate_smf

def gen_click_track_timings(bpm, n_beats, alt_click_delay_percent, sample_rate):
    seconds_per_beat = 1.0 / (bpm / 60.0)
    output_length_seconds = seconds_per_beat * n_beats
    beat_starts_seconds = [idx * seconds_per_beat for idx in range(n_beats)]

    output_nframes = int(output_length_seconds * sample_rate)
    beat_starts_frames = [int(start * sample_rate) for start in beat_starts_seconds]
    if alt_click_delay_percent > 0:
        for idx in range(n_beats):
            if idx % 2 > 0:
                # Off beats only
                beat_starts_frames[idx] += int(seconds_per_beat * sample_rate * alt_click_delay_percent / 100.0)

    return (output_nframes, beat_starts_frames)

# Generate click track MIDI data (internal SMF MIDI file format)
# Will generate a sequence of clicks by looping through the provided notes/channels/velocities.
# Every 2nd click is delayed by the click delay in percent (0 is no move, 100 is at the next click).
def gen_click_track_midi_smf(notes, channels, velocities, note_length, bpm, n_beats, alt_click_delay_percent):
    reference_sample_rate = 48000
    note_length_samples = note_length * reference_sample_rate
    (output_nframes, beat_starts) = gen_click_track_timings(bpm, n_beats, alt_click_delay_percent, reference_sample_rate)

    msgs = []
    for idx, beat in enumerate(beat_starts):
        chan = channels[idx % len(channels)]
        note = notes[idx % len(notes)]
        vel = velocities[idx % len(velocities)]
        msgs.append({
            'time': beat,
            'data': noteOn(chan, note, vel)
        })
        msgs.append({
            'time': beat + note_length_samples,
            'data': noteOff(chan, note, vel)
        })
        msgs = sorted(msgs, key=lambda m: m['time'])
    
    return generate_smf(msgs, output_nframes, reference_sample_rate)


# Generate a click track into a .wav file.
# Will generate a sequence of clicks by looping through the provided filenames.
# Every 2nd click is delayed by the click delay in percent (0 is no move, 100 is at the next click).
def gen_click_track_audio(click_wav_filenames, output_filename, bpm, n_beats, alt_click_delay_percent):
    wavs = dict()

    # Read input files
    for filename in click_wav_filenames:
        if filename in wavs.keys():
            continue

        # Read click
        with wave.open(filename, mode = 'rb') as wav:
            params = wav.getparams()
            wav_bytes = wav.readframes(params.nframes)
            wavs[filename] = dict()
            wavs[filename]['frames'] = [int.from_bytes(wav_bytes[idx*params.sampwidth : (idx+1)*params.sampwidth], 'little')
                                        for idx in range(params.nframes * params.nchannels)]
            wavs[filename]['params'] = params

    # Check that shared parameters are equal for all input wavs
    def shared_params_equal(a, b):
        return a.nchannels == b.nchannels and \
               a.sampwidth == b.sampwidth and \
               a.framerate == b.framerate
    reference_params = wavs[list(wavs.keys())[0]]['params']
    for wav in wavs.values():
        if not shared_params_equal(reference_params, wav['params']):
            raise Exception('Found differing parameters for the given input wavs')

    (output_nframes, beat_starts_frames) = gen_click_track_timings(bpm, n_beats, alt_click_delay_percent, reference_params.framerate)

    # Calculate output frames (interlaced)
    output_frames = [0] * output_nframes * reference_params.nchannels
    current_beat = 0
    # Note: framenr here is in each time frame (1 / samplerate). However, the input data
    # is in interlaced format with the channels mixed.
    for framenr in range(output_nframes):
        # Determine the current beat
        while current_beat < (n_beats-1) and beat_starts_frames[current_beat+1] <= framenr:
            current_beat = current_beat + 1

        # Determine the sample
        wav = wavs[click_wav_filenames[current_beat % len(click_wav_filenames)]]
        click_frame = framenr - beat_starts_frames[current_beat]
        if click_frame < (wav['params'].nframes / wav['params'].nchannels):
            for chan in range(wav['params'].nchannels):
                output_frames[framenr * wav['params'].nchannels + chan] = wav['frames'][click_frame * wav['params'].nchannels + chan]
    output_frames_bytes = b"".join([frame.to_bytes(reference_params.sampwidth, 'little') for frame in output_frames])

    # Write output frames
    with wave.open(output_filename, mode = 'wb') as output_wav:
        output_wav.setnchannels(reference_params.nchannels)
        output_wav.setsampwidth(reference_params.sampwidth)
        output_wav.setframerate(reference_params.framerate)
        output_wav.writeframes(output_frames_bytes)

# Standalone usage of this script
if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('output_wav')
    parser.add_argument('bpm')
    parser.add_argument('n_beats')
    parser.add_argument('alt_click_delay_percent')
    parser.add_argument('click_wavs', nargs='*')
    args = parser.parse_args()

    gen_click_track_audio(args.click_wavs, args.output_wav, float(args.bpm), int(args.n_beats), int(args.alt_click_delay_percent))
