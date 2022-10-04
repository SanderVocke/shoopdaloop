from PyQt6.QtCore import QObject, pyqtSlot
from ..GenClickTrack import gen_click_track
import os
import tempfile
import playsound

class ClickTrackGenerator(QObject):
    script_dir = os.path.dirname(os.path.abspath(__file__))
    clicks = {
        'click_high': script_dir + '/../../resources/click_high.wav',
        'click_low': script_dir + '/../../resources/click_low.wav',
        'shaker_primary': script_dir + '/../../resources/shaker_primary.wav',
        'shaker_secondary': script_dir + '/../../resources/shaker_secondary.wav'
    }

    @pyqtSlot(result=list)
    def get_possible_clicks(self):
        return list(self.clicks.keys())

    @pyqtSlot(list, int, int, int, result=str)
    def generate(self, click_names, bpm, n_beats, alt_click_delay_percent):
        click_filenames = [self.clicks[name] for name in click_names]
        out = tempfile.mkstemp()[1]

        gen_click_track(click_filenames, out, bpm, n_beats, alt_click_delay_percent)
        return out

    @pyqtSlot(str)
    def preview(self, wav_filename):
        playsound.playsound(wav_filename)

