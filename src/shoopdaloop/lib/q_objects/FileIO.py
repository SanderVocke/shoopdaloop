import os
import shutil
import tempfile
import tarfile
import time
from threading import Thread
import soundfile as sf
import numpy as np
import samplerate
import mido
import math
import glob
import json

from PySide6.QtCore import QObject, Slot, Signal, QThread
from PySide6.QtQml import QJSValue

from .Task import Task
from .Tasks import Tasks

from ..logging import Logger
from ..smf import generate_smf, parse_smf

def call_callable(callable, *args):
    if isinstance(callable, QJSValue):
        return callable.call(args)
    else:
        return callable(*args)

# Allow filesystem operations from QML
class FileIO(QThread):
    def __init__(self, parent=None):
        super(FileIO, self).__init__(parent)
        self.logger = Logger("Frontend.FileIO")

    startSavingFile = Signal()
    doneSavingFile = Signal()
    startLoadingFile = Signal()
    doneLoadingFile = Signal()
    
    ######################
    # SLOTS
    ######################
    
    @Slot(result=str)
    def get_current_directory(self):
        return os.getcwd()
    
    @Slot(result=str)
    def get_installation_directory(self):
        return os.path.dirname(os.path.realpath(__file__)) + '/../..'

    @Slot(str, str)
    def write_file(self, filename, content):
        with open(filename, 'w') as file:
            file.write(content)

    @Slot(str, result=str)
    def read_file(self, filename):
        r = None
        with open(filename, 'r') as file:
            r = file.read()
        return r
    
    @Slot(result=str)
    def create_temporary_folder(self):
        return tempfile.mkdtemp()
    
    @Slot(result=str)
    def create_temporary_file(self):
        return tempfile.mkstemp()[1]
    
    @Slot(result=str)
    def generate_temporary_filename(self):
        return tempfile.gettempdir() + '/' + next(tempfile._get_candidate_names())

    @Slot(str)
    def delete_recursive(self, folder):
        shutil.rmtree(folder)
    
    @Slot(str)
    def delete_file(self, filename):
        os.remove(filename)

    @Slot(str, str, bool)
    def make_tarfile(self, filename, source_dir, compress):
        flags = ("w:gz" if compress else "w")
        with tarfile.open(filename, flags) as tar:
            tar.add(source_dir, arcname='/')
    
    @Slot(str, str)
    def extract_tarfile(self, filename, target_dir):
        flags = "r:*"
        with tarfile.open(filename, flags) as tar:
            tar.extractall(target_dir)

    @Slot(str, int, list)
    def save_data_to_soundfile(self, filename, sample_rate, data):
        self.startSavingFile.emit()
        try:
            lengths = set()
            for d in data:
                lengths.add(len(d))
            if len(lengths) > 1:
                self.logger.error(lambda: 'Cannot save audio: channel lengths are not equal ({})'.format(list(lengths)))
                return
            # Soundfile wants NcxNs, not NsxNc
            _data = np.swapaxes(data, 0, 1)
            sf.write(filename, _data, sample_rate)
        finally:
            self.doneSavingFile.emit()

    @Slot(str, int, list)
    def save_channels_to_soundfile(self, filename, sample_rate, channels):
        datas = [c.get_data() for c in channels]
        self.save_data_to_soundfile(filename, sample_rate, datas)
        self.logger.info(lambda: "Saved {}-channel audio to {} ({} samples)".format(len(channels), filename, len(datas[0])))
    
    @Slot(str, int, 'QVariant')
    def save_channel_to_midi(self, filename, sample_rate, channel):
        self.startSavingFile.emit()
        try:
            msgs = channel.get_data()
            if os.path.splitext(filename)[1] == '.smf':
                with open(filename, 'w') as f:
                    f.write(json.dumps(generate_smf(msgs, channel.data_length, sample_rate), indent=2))
            else:
                mido_track = mido.MidiTrack()
                mido_file = mido.MidiFile()
                mido_file.tracks.append(mido_track)
                current_tick = 0

                for msg in msgs:
                    beat_length_s = mido.bpm2tempo(120) / 1000000.0
                    abstime_s = msg['time'] / float(sample_rate)
                    abstime_ticks = int(abstime_s / beat_length_s * mido_file.ticks_per_beat)
                    d_ticks = abstime_ticks - current_tick
                    mido_msg = mido.Message.from_bytes(bytes(msg['data']))
                    mido_msg.time = d_ticks
                    self.logger.trace(lambda: "save MIDI msg: {} (time={}, abs_s={}, abs={}, bl={}, tpb={})".format(mido_msg, d_ticks, abstime_s, abstime_ticks, beat_length_s, mido_file.ticks_per_beat))
                    if not mido_msg.is_meta and not mido_msg.is_realtime:
                        current_tick += d_ticks
                        mido_track.append(mido_msg)
                
                # TODO: append an End-Of-Track message to determine the length
                
                mido_file.save(filename)
            self.logger.info(lambda: "Saved MIDI channel to {} ({} messages)".format(filename, len(msgs)))
        finally:
            self.doneSavingFile.emit()
    
    @Slot(str, int, 'QVariant', result=Task)
    def save_channel_to_midi_async(self, filename, sample_rate, channel):
        task = Task(parent=self)
        def do_save():
            try:
                self.save_channel_to_midi(filename, sample_rate, channel)
            finally:
                task.done()
        
        t = Thread(target=do_save)
        t.start()
        return task

    @Slot(str, result=str)
    def basename(self, path):
        return os.path.basename(path)
    
    @Slot(str, result=bool)
    def is_absolute(self, path):
        return os.path.isabs(path)

    @Slot(str, result=str)
    def realpath(self, path):
        return os.path.realpath(path)

    @Slot(str, result=bool)
    def exists(self, path):
        return os.path.exists(path)
    
    @Slot(str, int, 'QVariant', 'QVariant', 'QVariant', 'QVariant')
    def load_midi_to_channels(self, 
                             filename,
                             sample_rate,
                             channels,
                             maybe_set_n_preplay_samples,
                             maybe_set_start_offset,
                             maybe_update_loop_to_datalength):
        self.startLoadingFile.emit()
        try:
            backend_msgs = []
            total_sample_time = 0
            if os.path.splitext(filename)[1] == '.smf':
                contents = None
                with open(filename, 'r') as f:
                    contents = f.read()
                (msgs, samples) = parse_smf(json.loads(contents), sample_rate)
                backend_msgs = msgs
                total_sample_time = samples
            else:
                mido_file = mido.MidiFile(filename)
                mido_msgs = [msg for msg in mido_file]
                self.logger.trace(lambda: "loading {} messages".format(len(mido_msgs)))
                total_time = 0.0
                
                for msg in mido_msgs:
                    msg_bytes = msg.bytes()
                    total_time += msg.time

                    if msg.is_meta or msg.bytes()[0] == 0xFF:
                        continue
                        
                    total_sample_time = int(total_time * sample_rate)
                    self.logger.trace(lambda: "load MIDI msg: {} (time={})".format(msg, total_sample_time))
                    backend_msgs.append({
                        'time': total_sample_time,
                        'data': [int(byte) for byte in msg_bytes]
                    })
            
            if isinstance(channels, QJSValue):
                channels = channels.toVariant()
            for channel in channels:
                channel.load_data(backend_msgs)            
                if maybe_set_start_offset != None:
                    channel.set_start_offset(maybe_set_start_offset)
                if maybe_set_n_preplay_samples != None:
                    channel.set_n_preplay_samples(maybe_set_n_preplay_samples)
            
            if maybe_update_loop_to_datalength:
                maybe_update_loop_to_datalength.set_length(total_sample_time)
            
            self.logger.info(lambda: "Loaded MIDI from {} into channel ({} messages, {} samples)".format(filename, len(backend_msgs), total_sample_time))
        finally:
            self.doneLoadingFile.emit()
    
    @Slot(str, int, 'QVariant', 'QVariant', 'QVariant', 'QVariant', result=Task)
    def load_midi_to_channels_async(self, 
                                   filename,
                                   sample_rate,
                                   channels,
                                   maybe_set_n_preplay_samples,
                                   maybe_set_start_offset,
                                   maybe_update_loop_to_datalength):
        task = Task(parent=self)
        def do_load():
            try:
                self.load_midi_to_channels(filename, sample_rate, channels, maybe_set_n_preplay_samples,
                                   maybe_set_start_offset, maybe_update_loop_to_datalength)
            finally:
                task.done()
        
        t = Thread(target=do_load)
        t.start()
        return task
    
    @Slot(str, int, list, result=Task)
    def save_channels_to_soundfile_async(self, filename, sample_rate, channels):
        task = Task(parent=self)
        def do_save():
            try:
                self.save_channels_to_soundfile(filename, sample_rate, channels)
            finally:
                task.done()
        
        t = Thread(target=do_save)
        t.start()
        return task
    
    @Slot(str, int, 'QVariant', list, 'QVariant', 'QVariant', 'QVariant')
    def load_soundfile_to_channels(
        self, 
        filename, 
        target_sample_rate, 
        maybe_target_data_length, 
        channels_to_loop_channels, 
        maybe_set_n_preplay_samples,
        maybe_set_start_offset,
        maybe_update_loop_to_datalength
    ):
        self.startLoadingFile.emit()
        try:
            data, file_sample_rate = sf.read(filename, dtype='float32')
            if data.ndim == 1:
                # Mono
                data = np.expand_dims(data, axis=1)

            target_sample_rate = int(target_sample_rate)
            file_sample_rate = int(file_sample_rate)
            resampled = data
            if target_sample_rate != file_sample_rate:
                self.logger.debug(lambda: "Resampling {} from {} to {}".format(filename, file_sample_rate, target_sample_rate))
                self.logger.trace(lambda: "Data shape before resample: {}".format(data.shape))
                ratio = target_sample_rate / file_sample_rate
                resampled = samplerate.resample(data, ratio, 'sinc_fastest')
                self.logger.trace(lambda: "Data shape after resample: {}".format(resampled.shape))

            if len(channels_to_loop_channels) > len(resampled):
                self.logger.error(lambda: "Need {} channels, but loaded file only has {}".format(len(channels_to_loop_channels), len(resampled)))
                return

            if maybe_target_data_length != None:
                prev_shape = resampled.shape
                new_shape = (maybe_target_data_length, prev_shape[1])
                resampled = np.resize(resampled, new_shape)
                self.logger.trace(lambda: "Data shape after resize: {}".format(resampled.shape))

            # We work with separate channel arrays
            resampled = np.swapaxes(resampled, 0, 1)

            for idx, data_channel in enumerate(resampled):
                channels = channels_to_loop_channels[idx]
                for channel in channels:
                    channel.load_data(data_channel)
                    channel.update() # dbg
                    if maybe_set_start_offset != None:
                        channel.set_start_offset(maybe_set_start_offset)
                    if maybe_set_n_preplay_samples != None:
                        channel.set_n_preplay_samples(maybe_set_n_preplay_samples)
                    self.logger.debug(lambda: "load channel: {} samples, result {}".format(len(data_channel), channel.data_length))  
                    
            if maybe_update_loop_to_datalength != None:
                maybe_update_loop_to_datalength.set_length(len(resampled[0]))
       
            self.logger.info(lambda: "Loaded {}-channel audio from {} ({} samples)".format(len(resampled), filename, len(resampled[0])))
        finally:
            self.doneLoadingFile.emit()
    
    @Slot(str, int, 'QVariant', list, 'QVariant', 'QVariant', 'QVariant', result=Task)
    def load_soundfile_to_channels_async(
        self, 
        filename, 
        target_sample_rate, 
        maybe_target_data_length, 
        channels_to_loop_channels, 
        maybe_set_n_preplay_samples,
        maybe_set_start_offset,
        maybe_update_loop_to_datalength
    ):
        task = Task(parent=self)
        def do_load():
            try:
                self.load_soundfile_to_channels(
                    filename, 
                    target_sample_rate, 
                    maybe_target_data_length, 
                    channels_to_loop_channels, 
                    maybe_set_n_preplay_samples,
                    maybe_set_start_offset,
                    maybe_update_loop_to_datalength
                )
            finally:
                task.done()
        
        t = Thread(target=do_load)
        t.start()
        return task
    
    @Slot(str, result='QVariant')
    def get_soundfile_info(self, filename):
        info = sf.info(filename)
        return {
            'channels': info.channels,
            'samplerate': info.samplerate,
            'frames': info.frames
        }
    
    @Slot(result='QVariant')
    def get_soundfile_formats(self):
        return dict(sf.available_formats())
    
    @Slot(str, bool, result='QVariant')
    def glob(self, pattern, recursive):
        return glob.glob(pattern, recursive=recursive)
