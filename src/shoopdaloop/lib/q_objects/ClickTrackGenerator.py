import os
import tempfile
import sounddevice as sd
import soundfile as sf
import json

from PySide6.QtCore import QObject, Slot
from .ShoopPyObject import *

from ..gen_click_track import gen_click_track_audio, gen_click_track_midi_smf

def play_wav(filename):
    data, fs = sf.read(filename, dtype='float32')
    sd.play(data, fs)

class ClickTrackGenerator(ShoopQObject):
    script_dir = os.path.dirname(os.path.abspath(__file__))
    clicks = {
        'click_high': script_dir + '/../../resources/click_high.wav',
        'click_low': script_dir + '/../../resources/click_low.wav',
        'shaker_primary': script_dir + '/../../resources/shaker_primary.wav',
        'shaker_secondary': script_dir + '/../../resources/shaker_secondary.wav'
    }

    @ShoopSlot(result=list)
    def get_possible_clicks(self):
        return list(self.clicks.keys())

    @ShoopSlot(list, int, int, int, result=str)
    def generate_audio(self, click_names, bpm, n_beats, alt_click_delay_percent):
        click_filenames = [self.clicks[name] for name in click_names]
        out = tempfile.mkstemp()[1]

        gen_click_track_audio(click_filenames, out, bpm, n_beats, alt_click_delay_percent)
        return out

    @ShoopSlot(list, list, list, float, int, int, int, result=str)
    def generate_midi(self, notes, channels, velocities, note_length, bpm, n_beats, alt_click_delay_percent):
        smf_data = gen_click_track_midi_smf(notes, channels, velocities, note_length, bpm, n_beats, alt_click_delay_percent)
        out = tempfile.mkstemp(suffix='.smf')[1]

        with open(out, 'w') as f:
            f.write(json.dumps(smf_data, indent=2))
        return out

    @ShoopSlot(str)
    def preview(self, wav_filename):
        play_wav(wav_filename)

