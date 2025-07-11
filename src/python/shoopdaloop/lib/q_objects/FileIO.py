import os
import shutil
import tempfile
import tarfile
import time
from threading import Thread
import soundfile as sf
import numpy as np
import mido
import math
import glob
import json
import inspect

from PySide6.QtCore import QObject, Slot, Signal, QThread, QMetaObject, Qt, Q_ARG
from PySide6.QtQml import QJSValue

from .Task import Task
from .Tasks import Tasks
from .ShoopPyObject import *
from shoop_config import shoop_qml_dir, shoop_lua_dir, shoop_resource_dir, shoop_schemas_dir
from ..recursive_jsvalue_convert import recursively_convert_jsvalue

from shoopdaloop.lib.logging import Logger
from shoopdaloop.lib.smf import generate_smf, parse_smf
import shoopdaloop.lib.backend_wrappers as backend_wrappers

import shoop_py_backend

def call_callable(callable, *args):
    if isinstance(callable, QJSValue):
        return callable.call(args)
    else:
        return callable(*args)

# Allow filesystem operations from QML
class FileIO(ShoopQObject):
    def __init__(self, parent=None):
        super(FileIO, self).__init__(parent)
        self.logger = Logger("Frontend.FileIO")
    
    ######################
    # SLOTS
    ######################
    
    @ShoopSlot(result=str)
    def get_qml_directory(self):
        return shoop_qml_dir
    
    @ShoopSlot(result=str)
    def get_lua_directory(self):
        return shoop_lua_dir
    
    @ShoopSlot(result=str)
    def get_resource_directory(self):
        return shoop_resource_dir
    
    @ShoopSlot(result=str)
    def get_schemas_directory(self):
        return shoop_schemas_dir
    
    def save_data_to_soundfile_impl(self, filename, sample_rate, data):
        try:
            lengths = set()
            list_data = []
            for d in data:
                if isinstance(d, shoop_py_backend.AudioChannelData):
                    list_data.append(d.data)
                elif isinstance(d, list):
                    list_data.append(d)
                else:
                    self.logger.error(lambda: 'Cannot save audio: unexpected data format')
                lengths.add(len(list_data[-1]))
            if len(lengths) > 1:
                self.logger.error(lambda: 'Cannot save audio: channel lengths are not equal ({})'.format(list(lengths)))
                return
            # Soundfile wants NcxNs, not NsxNc
            arr = np.array(list_data)
            arr = np.swapaxes(arr, 0, 1)
            sf.write(filename, arr, sample_rate)
        finally:
            pass
        
    @ShoopSlot(str, int, list)
    def save_data_to_soundfile(self, filename, sample_rate, data):
        self.save_data_to_soundfile_impl(filename, sample_rate, data)

    def save_channels_to_soundfile_impl(self, filename, sample_rate, channels):
        datas = [c.get_data() for c in channels]
        self.save_data_to_soundfile_impl(filename, sample_rate, datas)
        def length(data):
            if isinstance(data, shoop_py_backend.AudioChannelData):
                return len(data.data)
            return len(data)
        self.logger.info(lambda: "Saved {}-channel audio to {} ({} samples)".format(len(channels), filename, length(datas[0])))
    
    def save_channel_to_midi_impl(self, filename, sample_rate, channel):
        try:
            msgs = channel.get_all_midi_data()
            recorded_msgs = [m for m in msgs if m['time'] >= 0]
            state_msgs = [m for m in msgs if m['time'] < 0]
            if os.path.splitext(filename)[1] == '.smf':
                with open(filename, 'w') as f:
                    f.write(json.dumps(generate_smf(msgs, channel._data_length, sample_rate), indent=2))
                self.logger.info(lambda: "Saved MIDI channel to {} ({} messages, {} state messages)".format(filename, len(recorded_msgs), len(state_msgs)))
            else:
                mido_track = mido.MidiTrack()
                mido_file = mido.MidiFile()
                mido_file.tracks.append(mido_track)
                current_tick = 0

                for msg in recorded_msgs:
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
                self.logger.info(lambda: "Saved MIDI channel to {} ({} messages, discarded {} state messages)".format(filename, len(recorded_msgs), len(state_msgs)))
        finally:
            pass
    
    @ShoopSlot(str, int, 'QVariant', result=Task)
    def save_channel_to_midi_async(self, filename, sample_rate, channel):
        task = Task(parent=self)
        def do_save():
            try:
                self.save_channel_to_midi_impl(filename, sample_rate, channel)
            finally:
                QMetaObject.invokeMethod(task, 'done', Qt.QueuedConnection)
        
        t = Thread(target=do_save)
        t.start()
        return task

    @ShoopSlot(str, int, 'QVariant')
    def save_channel_to_midi(self, filename, sample_rate, channel):
        self.save_channel_to_midi_impl(filename, sample_rate, channel)

    def load_midi_to_channels_impl(self, 
                             filename,
                             sample_rate,
                             channels,
                             maybe_set_n_preplay_samples,
                             maybe_set_start_offset,
                             maybe_update_loop_to_datalength):
        try:
            if sample_rate:
                sample_rate = int(sample_rate)
            if maybe_set_n_preplay_samples:
                maybe_set_n_preplay_samples = int(maybe_set_n_preplay_samples)
            if maybe_set_start_offset:
                maybe_set_start_offset = int(maybe_set_start_offset)
            
            backend_msgs = []
            total_sample_time = 0
            if os.path.splitext(filename)[1] == '.smf':
                contents = None
                with open(filename, 'r') as f:
                    contents = f.read()
                (recorded_msgs, state_msgs, samples) = parse_smf(json.loads(contents), sample_rate)
                backend_msgs = [{'time': -1, 'data': m} for m in state_msgs] + recorded_msgs
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
                channels = recursively_convert_jsvalue(channels)
            for channel in channels:
                channel.load_all_midi_data(backend_msgs)        
                if maybe_set_start_offset != None:
                    channel.set_start_offset(maybe_set_start_offset)
                if maybe_set_n_preplay_samples != None:
                    channel.set_n_preplay_samples(maybe_set_n_preplay_samples)
            
            if maybe_update_loop_to_datalength:
                QMetaObject.invokeMethod(maybe_update_loop_to_datalength, 'queue_set_length_qml', Qt.DirectConnection, Q_ARG('QVariant', total_sample_time))
            
            self.logger.info(lambda: "Loaded MIDI from {} into channel ({} recorded messages, {} state messages, {} samples)".format(
                filename,
                len([m for m in backend_msgs if m['time'] >= 0]),
                len([m for m in backend_msgs if m['time'] < 0]),
                total_sample_time
            ))
        finally:
            pass
    
    @ShoopSlot(str, int, 'QVariant', 'QVariant', 'QVariant', 'QVariant', result=Task)
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
                self.load_midi_to_channels_impl(filename, sample_rate, channels, maybe_set_n_preplay_samples,
                                   maybe_set_start_offset, maybe_update_loop_to_datalength)
            finally:
                QMetaObject.invokeMethod(task, 'done', Qt.QueuedConnection)
        
        t = Thread(target=do_load)
        t.start()
        return task
    
    @ShoopSlot(str, int, 'QVariant', 'QVariant', 'QVariant', 'QVariant')
    def load_midi_to_channels(self, 
                                filename,
                                sample_rate,
                                channels,
                                maybe_set_n_preplay_samples,
                                maybe_set_start_offset,
                                maybe_update_loop_to_datalength):
        self.load_midi_to_channels_impl(filename, sample_rate, channels, maybe_set_n_preplay_samples, maybe_set_start_offset, maybe_update_loop_to_datalength)
    
    @ShoopSlot(str, int, list, result=Task)
    def save_channels_to_soundfile_async(self, filename, sample_rate, channels):        
        task = Task(parent=self)
        def do_save():
            try:
                self.save_channels_to_soundfile_impl(filename, sample_rate, channels)
            finally:
                QMetaObject.invokeMethod(task, 'done', Qt.QueuedConnection)
        
        t = Thread(target=do_save)
        t.start()
        return task

    @ShoopSlot(str, int, list)
    def save_channels_to_soundfile(self, filename, sample_rate, channels):
        self.save_channels_to_soundfile_impl(filename, sample_rate, channels)
    
    def load_soundfile_to_channels_impl(
        self, 
        filename, 
        target_sample_rate, 
        maybe_target_data_length, 
        channels_to_loop_channels, 
        maybe_set_n_preplay_samples,
        maybe_set_start_offset,
        maybe_update_loop_to_datalength
    ):
        try:
            if maybe_target_data_length:
                maybe_target_data_length = int(maybe_target_data_length)
            if maybe_set_start_offset:
                maybe_set_start_offset = int(maybe_set_start_offset)
            if maybe_set_n_preplay_samples:
                maybe_set_n_preplay_samples = int(maybe_set_n_preplay_samples)

            data, file_sample_rate = sf.read(filename, dtype='float32')
            if data.ndim == 1:
                # Mono
                data = np.expand_dims(data, axis=1)

            target_sample_rate = int(target_sample_rate)
            file_sample_rate = int(file_sample_rate)
            if target_sample_rate == file_sample_rate:
                resampled = data
            else:
                self.logger.debug(lambda: "Resampling {} from {} to {}".format(filename, file_sample_rate, target_sample_rate))
                self.logger.trace(lambda: "Data shape before resample: {}".format(data.shape))
                target_n_frames = maybe_target_data_length
                if target_n_frames is None:
                    target_n_frames = int(target_sample_rate / file_sample_rate * data.shape[0])
                resampled = backend_wrappers.resample_audio(data, target_n_frames)
            
            self.logger.trace(lambda: "Data shape: {}".format(resampled.shape))

            if len(channels_to_loop_channels) > len(resampled):
                self.logger.error(lambda: "Need {} channels, but loaded file only has {}".format(len(channels_to_loop_channels), len(resampled)))
                return

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
                    self.logger.debug(lambda: f"load channel: {len(data_channel)} samples, resulting channel data length {channel.data_length}")  
                    
            if maybe_update_loop_to_datalength != None:
                QMetaObject.invokeMethod(maybe_update_loop_to_datalength, 'queue_set_length_qml', Qt.DirectConnection, Q_ARG('QVariant', len(resampled[0])))
       
            self.logger.info(lambda: "Loaded {}-channel audio from {} ({} samples)".format(len(resampled), filename, len(resampled[0])))
        finally:
            pass
    
    @ShoopSlot(str, int, 'QVariant', list, 'QVariant', 'QVariant', 'QVariant', result=Task)
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
                self.load_soundfile_to_channels_impl(
                    filename, 
                    target_sample_rate, 
                    maybe_target_data_length, 
                    channels_to_loop_channels, 
                    maybe_set_n_preplay_samples,
                    maybe_set_start_offset,
                    maybe_update_loop_to_datalength
                )
            finally:
                QMetaObject.invokeMethod(task, 'done', Qt.QueuedConnection)
        
        t = Thread(target=do_load)
        t.start()
        return task

    @ShoopSlot(str, int, 'QVariant', list, 'QVariant', 'QVariant', 'QVariant')
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
        self.load_soundfile_to_channels_impl(filename, target_sample_rate, maybe_target_data_length, channels_to_loop_channels, maybe_set_n_preplay_samples, maybe_set_start_offset, maybe_update_loop_to_datalength)
    
    @ShoopSlot(str, result='QVariant')
    def get_soundfile_info(self, filename):
        info = sf.info(filename)
        return {
            'channels': info.channels,
            'samplerate': info.samplerate,
            'frames': info.frames
        }
    
    @ShoopSlot(result='QVariant')
    def get_soundfile_formats(self):
        return dict(sf.available_formats())
