import os
import tempfile
import pyaudio as pa
import threading
import wave

from PySide6.QtCore import QObject, Slot
from .ShoopPyObject import *

from ..gen_click_track import gen_click_track

def play_wav(filename):
    with wave.open(filename, 'rb') as wav:
        p = pa.PyAudio()
        stream = p.open(format=p.get_format_from_width(wav.getsampwidth()),
                        channels=wav.getnchannels(),
                        rate=wav.getframerate(),
                        output=True)
        data = wav.readframes(1024)
        while data:
            stream.write(data)
            data = wav.readframes(1024)
        stream.stop_stream()
        stream.close()
        p.terminate()

class ClickTrackGenerator(ShoopQObject):
    script_dir = os.path.dirname(os.path.abspath(__file__))
    clicks = {
        'click_high': script_dir + '/../../resources/click_high.wav',
        'click_low': script_dir + '/../../resources/click_low.wav',
        'shaker_primary': script_dir + '/../../resources/shaker_primary.wav',
        'shaker_secondary': script_dir + '/../../resources/shaker_secondary.wav'
    }

    @Slot(result=list)
    def get_possible_clicks(self):
        return list(self.clicks.keys())

    @Slot(list, int, int, int, result=str)
    def generate(self, click_names, bpm, n_beats, alt_click_delay_percent):
        click_filenames = [self.clicks[name] for name in click_names]
        out = tempfile.mkstemp()[1]

        gen_click_track(click_filenames, out, bpm, n_beats, alt_click_delay_percent)
        return out

    @Slot(str)
    def preview(self, wav_filename):
        play_thread = threading.Thread(target=play_wav, args=(wav_filename,))
        play_thread.daemon = True
        play_thread.start()

