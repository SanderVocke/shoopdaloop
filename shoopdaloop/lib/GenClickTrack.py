import wave
import argparse

# Generate a click track into a .wav file.
# Will generate a sequence of clicks by looping through the provided filenames.
# Every 2nd click is delayed by the click delay in percent (0 is no move, 100 is at the next click).
def gen_click_track(click_wav_filenames, output_filename, bpm, n_beats, alt_click_delay_percent):
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

    # Calculate timings
    seconds_per_beat = 1.0 / (bpm / 60.0)
    output_length_seconds = seconds_per_beat * n_beats
    beat_starts_seconds = [idx * seconds_per_beat for idx in range(n_beats)]

    output_nframes = int(output_length_seconds * reference_params.framerate)
    beat_starts_frames = [int(start * reference_params.framerate) for start in beat_starts_seconds]
    if alt_click_delay_percent > 0:
        for idx in range(n_beats):
            if idx % 2 > 0:
                # Off beats only
                beat_starts_frames[idx] += int(seconds_per_beat * reference_params.framerate * alt_click_delay_percent / 100.0)

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

    gen_click_track(args.click_wavs, args.output_wav, float(args.bpm), int(args.n_beats), int(args.alt_click_delay_percent))
