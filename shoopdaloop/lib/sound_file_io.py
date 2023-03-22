import soundfile as sf
import numpy as np
import scipy as sp
import resampy

# Load an audio file, resample to the given sample rate and return
# as a list (channels) of lists (samples).
# On failure, None is returned.
def load_audio_file(filename, target_sample_rate, maybe_override_length=None):
    try:
        # Read
        np_data, sample_rate_in = sf.read(filename, dtype='float32')

        # Reshape
        if np_data.ndim == 1:
            np_data = [np_data]
        elif np_data.ndim == 2:
            # np_data is N_Samples elements of N_Channels numbers, swap that to
            # get per-channel arrays
            np_data = np.swapaxes(np_data, 0, 1)
        else:
            raise ValueError("Unexpected number of dimensions of sound data: {}".format(np_data.ndim))
        
        # Resample
        n_samples = len(np_data[0])
        target_n_samples = int(n_samples / float(sample_rate_in) * target_sample_rate)
        resampled = np_data
        if int(target_sample_rate) != int(sample_rate_in):
            for chan in resampled:
                chan = resampy.resample(chan, sample_rate_in, target_sample_rate)

        # Length override
        for chan in resampled:
            if maybe_override_length and maybe_override_length > len(chan):
                last_sample = chan[len(chan) - 1]
                for idx in range(maybe_override_length - target_n_samples):
                    chan.append(last_sample)
            if maybe_override_length and maybe_override_length < len(chan):
                chan = chan[:maybe_override_length]
        
        return resampled
    except Exception as e:
        print("Failed to load sound file: {}".format(format(e)))
        return None