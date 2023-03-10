from PyQt6.QtCore import QObject, pyqtSlot, pyqtSignal

import os
import shutil
import tempfile
import tarfile
import time
from threading import Thread
import soundfile as sf
import numpy as np
import resampy

# Allow text file I/O from QML
class FileIO(QObject):
    def __init__(self, parent=None):
        super(FileIO, self).__init__(parent)

    startSavingSoundfile = pyqtSignal()
    doneSavingSoundfile = pyqtSignal()
    startLoadingSoundfile = pyqtSignal()
    doneLoadingSoundfile = pyqtSignal()
    
    ######################
    # SLOTS
    ######################

    @pyqtSlot(str, str)
    def write_file(self, filename, content):
        with open(filename, 'w') as file:
            file.write(content)

    @pyqtSlot(str, result=str)
    def read_file(self, filename):
        r = None
        with open(filename, 'r') as file:
            r = file.read()
        return r
    
    @pyqtSlot(result=str)
    def create_temporary_folder(self):
        return tempfile.mkdtemp()
    
    @pyqtSlot(result=str)
    def create_temporary_file(self):
        return tempfile.mkstemp()[1]

    @pyqtSlot(str)
    def delete_recursive(self, folder):
        shutil.rmtree(folder)

    @pyqtSlot(str, str, bool)
    def make_tarfile(self, filename, source_dir, compress):
        flags = ("w:gz" if compress else "w")
        with tarfile.open(filename, flags) as tar:
            tar.add(source_dir, arcname=source_dir)
    
    @pyqtSlot(str, int, list)
    def save_channels_to_soundfile(self, filename, sample_rate, channels):
        self.startSavingSoundfile.emit()
        def do_save():
            try:
                datas = [c.get_data() for c in channels]
                lengths = set()
                for d in datas:
                    lengths.add(len(d))
                if len(lengths) > 1:
                    raise Exception('Cannot save audio: channel lengths are not equal ({})'.format(list(lengths)))
                # Soundfile wants NcxNs, not NsxNc
                data = np.swapaxes(datas, 0, 1)
                sf.write(filename, data, sample_rate)
                print("Saved {}-channel audio to {}".format(len(channels), filename))
            finally:
                self.doneSavingSoundfile.emit()
        
        t = Thread(target=do_save)
        t.start()
    
    @pyqtSlot(str, int, 'QVariant')
    def load_soundfile_to_channels(self, filename, target_sample_rate, map_file_channels_to_lists_of_loop_channels):
        self.startLoadingSoundfile.emit()
        def do_load():
            try:
                data, file_sample_rate = sf.read(filename, dtype='float32')
                if data.ndim == 1:
                    # Mono
                    data = [data]
                else:
                    # Sf gives NcxNs, we want NsxNc
                    data = np.swapaxes(data, 0, 1)
                n_file_samples = len(data[0])
                n_target_samples = int(float(n_file_samples) / float(file_sample_rate) * float(target_sample_rate))

                target_sample_rate = int(target_sample_rate)
                file_sample_rate = int(file_sample_rate)
                resampled = data
                if target_sample_rate != file_sample_rate:
                    resampled = resampy.resample(data, file_sample_rate, target_sample_rate)
                
                for (data_channel, idx) in enumerate(data):
                    channels = map_file_channels_to_lists_of_loop_channels[idx]
                    for channel in channels:
                        channel.load_data(data_channel)

                print("Loaded {}-channel audio from {}".format(len(data), filename))
            finally:
                self.doneLoadingSoundfile.emit()
        
        t = Thread(target=do_save)
        t.start()
    
    @pyqtSlot(str, result='QVariant')
    def get_soundfile_info(self, filename):
        info = sf.info(filename)
        return {
            'channels': info.channels,
            'samplerate': info.samplerate,
            'frames': info.frames
        }
    
    @pyqtSlot(result='QVariant')
    def get_soundfile_formats(self):
        return dict(sf.available_formats())