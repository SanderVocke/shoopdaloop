from ctypes import *

from PyQt6.QtCore import QObject, pyqtSignal, pyqtSlot, QTimer, pyqtProperty

from .NChannelAbstractLooperManager import NChannelAbstractLooperManager

import sys
sys.path.append('../..')

import lib.backend_interface as backend
from lib.StatesAndActions import *
from lib.state_helpers import *
from collections import OrderedDict
from third_party.pyjacklib import jacklib
from functools import partial, reduce
from threading import Thread

from .LooperState import LooperState
from .DryWetPairAbstractLooperManager import DryWetPairAbstractLooperManager
from .NChannelAbstractLooperManager import NChannelAbstractLooperManager
from .PortManager import PortManager
from .PortsManager import PortsManager

import time
import cProfile
import resampy

import soundfile as sf
import numpy as np
import scipy as sp
import shutil
import tempfile
import tarfile
from copy import copy

import mido

from pprint import *

class SlowMidiCallback():
    def get_cb(self):
        def c_cb(port, length, data):
            array_type = (c_ubyte * length)
            data_list = [data[i] for i in range(length)]
            self.cb(data_list)
        return backend.SlowMIDIReceivedCallback(c_cb)
    
    def __init__(self, cb):
        self.cb = cb
        self.c_cb = self.get_cb()

class BackendManager(QObject):
    newSessionStateStr = pyqtSignal(str)
    requestLoadSession = pyqtSignal(str)
    requestSaveSession = pyqtSignal(str)

    def __init__(self,
                 port_name_pairs,
                 mixed_output_port_names,
                 midi_port_name_pairs,
                 loops_to_ports_map,
                 loops_hard_sync_map,
                 loops_sync_map,
                 ports_to_mixed_outputs_map,
                 ports_midi_enabled_list,
                 max_loop_length_seconds,
                 client_name,
                 update_period_seconds,
                 parent=None):
        super(BackendManager, self).__init__(parent)

        self.update_cb = backend.UpdateCallback(self.update_cb)
        self.abort_cb = backend.AbortCallback(self.abort_cb)

        self.n_loops = len(loops_to_ports_map)
        self.n_ports = len(port_name_pairs)
        self.n_midi_ports = len(ports_midi_enabled_list)
        self.n_mixed_output_ports = len(mixed_output_port_names)
        self.client_name = client_name
        self.max_loop_length_seconds = max_loop_length_seconds
        self.port_name_pairs = port_name_pairs
        self.loops_to_ports = loops_to_ports_map
        self.loops_sync_map = loops_sync_map
        self.loops_hard_sync_map = loops_hard_sync_map
        self.ports_midi_enabled_list = ports_midi_enabled_list
        self.ports_to_mixed_outputs_map = ports_to_mixed_outputs_map
        self.mixed_output_port_names = mixed_output_port_names
        self.midi_port_name_pairs = midi_port_name_pairs

        self._channel_looper_managers = [
            LooperState(parent=self) for i in range(self.n_loops)
        ]
        for idx, looper in enumerate(self._channel_looper_managers):
            looper.set_get_waveforms_fn(lambda from_sample, to_sample, samples_per_bin, idx=idx: {
                'waveform': self.get_loop_rms(idx, from_sample, to_sample, samples_per_bin)
            })
            looper.set_get_midi_fn(lambda idx=idx: self.get_loop_midi_messages(idx))

        def channels_for_stereo(stereo_idx):
            return [stereo_idx*2, stereo_idx*2+1]

        def create_stereo_looper(idx):
            channel_loop_idxs = channels_for_stereo(idx)
            l = NChannelAbstractLooperManager(
                [self._channel_looper_managers[i] for i in channel_loop_idxs], parent=self)
            
            def load_from_file (filename):
                self.load_loops_from_file(channel_loop_idxs, filename, None)
                l.loadedData.emit()

            l.signalLoopAction.connect(lambda action, args, sync: self.do_backend_loops_action(channel_loop_idxs, action, args, sync))
            l.saveToFile.connect(lambda filename: self.save_loops_to_file(channel_loop_idxs, filename, False))
            l.loadFromFile.connect(lambda filename: load_from_file(filename))

            return l
        
        def dry_wet_for_logical(logical_idx):
            return [logical_idx*2, logical_idx*2+1]
        
        def channels_for_logical(logical_idx):
            dry_wet = dry_wet_for_logical(logical_idx)
            return channels_for_stereo(dry_wet[0]) + channels_for_stereo(dry_wet[1])
        
        self._stereo_looper_managers = [
            create_stereo_looper(idx) for idx in range(int(len(self._channel_looper_managers)/2))
        ]

        def create_logical_looper(idx):
            dry_wet = dry_wet_for_logical(idx)
            channels = channels_for_logical(idx)
            l = DryWetPairAbstractLooperManager(self._stereo_looper_managers[dry_wet[0]],
                                                self._stereo_looper_managers[dry_wet[1]],
                                                parent=self)

            def load_midi_file (filename):
                mgrs = [self._channel_looper_managers[idx] for idx in channels]
                new_length = self.load_loop_midi_from_file(channels_for_stereo(dry_wet[0])[0], filename)
                backend_idxs = (c_uint * len(channels))()
                for i,idx in enumerate(channels):
                    backend_idxs[i] = idx
                backend.set_loops_length(backend_idxs, len(channels), new_length)
                l.loadedData.emit()
            
            def save_midi_file (filename):
                self.save_loop_midi_to_file(channels_for_stereo(dry_wet_for_logical(idx)[0])[0], filename)
            
            l.loadMidiFile.connect(lambda filename: load_midi_file(filename))
            l.saveMidiFile.connect(lambda filename: save_midi_file(filename))
            
            return l
        
        self._logical_looper_managers = [
            create_logical_looper(idx) for idx in range(int(len(self._stereo_looper_managers)/2))
        ]

        self._channel_port_managers = [
            PortManager(parent=self) for i in range(self.n_ports)
        ]
        for idx, p in enumerate(self._channel_port_managers):
            p.signalPortAction.connect(lambda action_id, maybe_arg, idx=idx: self.do_port_action(idx, action_id, maybe_arg))

        def create_stereo_ports_manager(idx):
            p = PortsManager([
                self._channel_port_managers[idx*2],
                self._channel_port_managers[idx*2+1]
                ], parent=self)
            return p
        self._stereo_port_managers = [
            create_stereo_ports_manager(idx) for idx in range(int(len(self._channel_port_managers)/2))
        ]

        def create_track_ports_manager(idx):
            p = PortsManager([
                    self._stereo_port_managers[idx*2],
                    self._stereo_port_managers[idx*2+1]
                ], parent=self)
            return p
        self._track_port_managers = [
            create_track_ports_manager(idx) for idx in range(int(len(self._stereo_port_managers)/2))
        ]

        self.update_timer = QTimer()
        self.update_timer.setSingleShot(False)
        self.update_timer.setInterval(int(1000.0 * update_period_seconds))
        self.update_timer.timeout.connect(lambda: backend.request_update())
        self.update_timer.start()

        self._sample_rate = 0

        self._session_loading = False
        self._session_saving = False
        self._session_save_counter = 0
        self._session_save_failed_counter = 0
        self._session_load_counter = 0
        self._session_load_failed_counter = 0

    ######################
    # PROPERTIES
    ######################

    sessionLoadingChanged = pyqtSignal(bool)
    @pyqtProperty(bool, notify=sessionLoadingChanged)
    def session_loading(self):
        return self._session_loading
    @session_loading.setter
    def session_loading(self, s):
        if self._session_loading != s:
            self._session_loading = s
            self.sessionLoadingChanged.emit(s)
    
    sessionSavingChanged = pyqtSignal(bool)
    @pyqtProperty(bool, notify=sessionSavingChanged)
    def session_saving(self):
        return self._session_saving
    @session_saving.setter
    def session_saving(self, s):
        if self._session_saving != s:
            self._session_saving = s
            self.sessionSavingChanged.emit(s)

    sessionSaveCounterChanged = pyqtSignal(int)
    @pyqtProperty(bool, notify=sessionSaveCounterChanged)
    def session_save_counter(self):
        return self._session_save_counter
    @session_save_counter.setter
    def session_save_counter(self, s):
        if self._session_save_counter != s:
            self._session_save_counter = s
            self.sessionSaveCounterChanged.emit(s)
    
    sessionSaveFailedCounterChanged = pyqtSignal(int)
    @pyqtProperty(bool, notify=sessionSaveFailedCounterChanged)
    def session_save_failed_counter(self):
        return self._session_save_failed_counter
    @session_save_failed_counter.setter
    def session_save_failed_counter(self, s):
        if self._session_save_failed_counter != s:
            self._session_save_failed_counter = s
            self.sessionSaveFailedCounterChanged.emit(s)
    
    sessionLoadCounterChanged = pyqtSignal(int)
    @pyqtProperty(bool, notify=sessionLoadCounterChanged)
    def session_load_counter(self):
        return self._session_load_counter
    @session_load_counter.setter
    def session_load_counter(self, s):
        if self._session_load_counter != s:
            self._session_load_counter = s
            self.sessionLoadCounterChanged.emit(s)
    
    sessionLoadFailedCounterChanged = pyqtSignal(int)
    @pyqtProperty(bool, notify=sessionLoadFailedCounterChanged)
    def session_load_failed_counter(self):
        return self._session_load_failed_counter
    @session_load_failed_counter.setter
    def session_load_failed_counter(self, s):
        if self._session_load_failed_counter != s:
            self._session_load_failed_counter = s
            self.sessionLoadFailedCounterChanged.emit(s)
    
    sampleRateChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=sampleRateChanged)
    def sample_rate(self):
        return self._sample_rate
    @sample_rate.setter
    def sample_rate(self, s):
        if self._sample_rate != s:
            self._sample_rate = s
            self.sampleRateChanged.emit(s)
    
    # Looper managers are built up in a hierarchy.
    # From top to bottom:
    # - logical_looper_managers: managers of the N abstract loopers
    #     which manage a pair of stereo loopers: one for the
    #     dry recording and one for the wet recording.
    #     These are the loopers as the user sees them.
    # - stereo_looper_managers: managers of each of the N*2 stereo
    #     loopers used for the dry and wet channels underneath.
    # - channel_looper_managers: managers of each of the N*4
    #     individual channel loopers underneath.

    channel_looper_managersChanged = pyqtSignal(list)
    @pyqtProperty(list, notify=channel_looper_managersChanged)
    def channel_looper_managers(self):
        return self._channel_looper_managers
    
    stereo_looper_managersChanged = pyqtSignal(list)
    @pyqtProperty(list, notify=stereo_looper_managersChanged)
    def stereo_looper_managers(self):
        return self._stereo_looper_managers
    
    logical_looper_managersChanged = pyqtSignal(list)
    @pyqtProperty(list, notify=logical_looper_managersChanged)
    def logical_looper_managers(self):
        return self._logical_looper_managers

    # A similar hierarchy is used for the ports
    channel_port_managersChanged = pyqtSignal(list)
    @pyqtProperty(list, notify=channel_port_managersChanged)
    def channel_port_managers(self):
        return self._channel_port_managers
    
    stereo_port_managersChanged = pyqtSignal(list)
    @pyqtProperty(list, notify=stereo_port_managersChanged)
    def stereo_port_managers(self):
        return self._stereo_port_managers
    
    track_port_managersChanged = pyqtSignal(list)
    @pyqtProperty(list, notify=track_port_managersChanged)
    def track_port_managers(self):
        return self._track_port_managers

    ######################
    # OTHER
    ######################
    
    def __enter__(self):
        print('Starting back-end.')
        self.initialize_backend()
        return self

    def __exit__(self, type, value, traceback):
        self.terminate()
    
    def terminate(self):
        print('Terminating back-end.')
        backend.terminate()
    
    def initialize_backend(self):
        loops_to_ports = (c_uint * self.n_loops)()
        loops_hard_sync_map = (c_int * self.n_loops)()
        loops_sync_map = (c_int * self.n_loops)()
        ports_to_mixed_outputs_map = (c_int * self.n_ports)()
        ports_midi_enabled = (c_uint * self.n_ports)()
        input_port_names = (POINTER(c_char) * self.n_ports)()
        output_port_names = (POINTER(c_char) * self.n_ports)()
        mixed_output_port_names = (POINTER(c_char) * self.n_mixed_output_ports)()
        midi_input_port_names = (POINTER(c_char) * self.n_midi_ports)()
        midi_output_port_names = (POINTER(c_char) * self.n_midi_ports)()

        # Construct the port names and loop mappings        
        for port_idx, item in enumerate(self.port_name_pairs):
            input_port_names[port_idx] = create_string_buffer(item[0].encode('ascii'))
            output_port_names[port_idx] = create_string_buffer(item[1].encode('ascii'))
        
        for midi_port_idx, item in enumerate(self.midi_port_name_pairs):
            midi_input_port_names[midi_port_idx] = create_string_buffer(item[0].encode('ascii'))
            midi_output_port_names[midi_port_idx] = create_string_buffer(item[1].encode('ascii'))
    
        for idx, name in enumerate(self.mixed_output_port_names):
            mixed_output_port_names[idx] = create_string_buffer(name.encode('ascii'))
        
        for i in range(self.n_loops):
            loops_to_ports[i] = self.loops_to_ports[i]
            loops_hard_sync_map[i] = self.loops_hard_sync_map[i]
            loops_sync_map[i] = self.loops_sync_map[i]
        
        for i in range(self.n_ports):
            ports_to_mixed_outputs_map[i] = self.ports_to_mixed_outputs_map[i]
        
        for i in range(self.n_ports):
            ports_midi_enabled[i] = 1 if i in self.ports_midi_enabled_list else 0
        
        do_profiling = os.getenv('SHOOPDALOOP_PROFILING') != None
        features = backend.backend_features_t(backend.Profiling) if do_profiling else 0

        _jack_client = backend.initialize(
            self.n_loops,
            self.n_ports,
            self.n_mixed_output_ports,
            self.max_loop_length_seconds,
            loops_to_ports,
            loops_hard_sync_map,
            loops_sync_map,
            ports_to_mixed_outputs_map,
            ports_midi_enabled,
            input_port_names,
            output_port_names,
            mixed_output_port_names,
            midi_input_port_names,
            midi_output_port_names,
            self.client_name.encode('ascii'),
            32768, # bit less than a second @ 48000
            self.update_cb,
            self.abort_cb,
            features
        )
        self.jack_client = cast(_jack_client, POINTER(jacklib.jack_client_t))
    
    def get_jack_input_port(self, idx):
        return self.convert_to_jack_port(backend.get_port_input_handle(idx))
    
    def remap_port_input(self, port, remapped):
        backend.remap_port_input(port, remapped)

    def reset_port_input_remap(self, port):
        backend.reset_port_input_remapping(port)

    def convert_to_jack_port(self, backend_port):
        return cast(backend_port, POINTER(jacklib.jack_port_t))

    def update_cb(
                self,
                n_loops,
                n_ports,
                sample_rate,
                loop_states,
                loop_next_modes,
                loop_next_mode_countdowns,
                loop_lengths,
                loop_positions,
                loop_volumes,
                port_volumes,
                port_passthrough_levels,
                port_latencies,
                ports_muted,
                port_inputs_muted,
                loop_output_peaks,
                port_output_peaks,
                port_input_peaks,
                loop_n_output_events,
                port_n_input_events,
                port_n_output_events):
        # pr = cProfile.Profile()
        # pr.enable()
        self.sample_rate = sample_rate

        for i in range(n_loops):
            m = self._channel_looper_managers[i]
            
            # Determine if the loop cycled
            cycled = False
            if is_playing_state(m.mode) and is_playing_state(loop_states[i]) and \
               m.length > 0 and \
               (loop_positions[i] < m.position):
               cycled = True
            
            # Determine if the loop passed the halfway point
            passed_halfway = False
            if is_playing_state(m.mode) and is_playing_state(loop_states[i]) and \
               m.length > 0 and \
               (loop_positions[i] >= m.length / 2) and \
               (m.position < m.length / 2):
               passed_halfway = True
               
            m.mode = loop_states[i]
            m.next_mode = loop_next_modes[i]
            m.next_mode_countdown = loop_next_mode_countdowns[i]
            m.length = loop_lengths[i]
            m.position = loop_positions[i]
            m.volume = loop_volumes[i]
            m.outputPeak = loop_output_peaks[i]

            if cycled:
                m.cycled.emit()
            if passed_halfway:
                m.passed_halfway.emit()

        for i in range(n_ports):
            p = self._channel_port_managers[i]
            p.set_volume_direct(port_volumes[i])
            p.set_passthrough_direct(port_passthrough_levels[i])
            p.set_muted_direct(ports_muted[i] != 0)
            p.set_passthroughMuted_direct(port_inputs_muted[i] != 0)
            p.set_recordingLatency_direct(port_latencies[i] / self.sample_rate)
            p.outputPeak = port_output_peaks[i]
            p.inputPeak = port_input_peaks[i]
        
        # # TODO port changes
        # pr.disable()
        # pr.print_stats()
        return 0

    def abort_cb(self):
        print("Backend aborted.")
        exit(1)
    
    @pyqtSlot(list, int, list, bool)
    def do_backend_loops_action(self, loop_idxs, action_id, maybe_args, with_sync):
        for loop_idx in loop_idxs:
            if loop_idx < 0 or loop_idx >= self.n_loops:
                raise ValueError("Backend: loop idx out of range")
        
        if action_id < 0 or \
           action_id >= backend.LOOP_ACTION_MAX:
           raise ValueError("Backend: loop action {} is not implemented in back-end".format(action_id))
        
        idxs_data = (c_uint * len(loop_idxs))(*loop_idxs)
        args_data = (c_float * len(maybe_args))(*maybe_args)
        
        backend.do_loop_action(
            idxs_data,
            len(loop_idxs),
            backend.loop_action_t(action_id),
            args_data,
            len(maybe_args),
            with_sync
        )

    @pyqtSlot(int, list)
    def load_loop_data(self, loop_idx, data):
        c_data = (c_float * len(data))(*data)
        backend.load_loop_data(loop_idx, len(data), c_data)
    
    @pyqtSlot(int, result=list)
    def get_loop_data(self, loop_idx, do_stop=True):
        c_float_p = POINTER(c_float)
        c_data = c_float_p()
        n_floats = backend.get_loop_data(loop_idx, byref(c_data), int(bool(do_stop)))
        data = [float(c_data[i]) for i in range(n_floats)]
        backend.shoopdaloop_free(cast(c_data, c_void_p))
        c_data = None
        return data
    
    @pyqtSlot(int, int, int, int, result=list)
    def get_loop_rms(self, loop_idx, from_sample, to_sample, samples_per_bin):
        c_float_p = POINTER(c_float)
        c_data = c_float_p()
        n_floats = backend.get_loop_data_rms(loop_idx, from_sample, to_sample, samples_per_bin, byref(c_data))        
        data = [float(c_data[i]) for i in range(n_floats)]
        backend.shoopdaloop_free(cast(c_data, c_void_p))
        c_data = None
        return data
    
    @pyqtSlot(int, int, float)
    def do_port_action(self, port_idx, action_id, maybe_arg):
        if action_id < 0 or action_id >= backend.PORT_ACTION_MAX:
            raise ValueError("Backend: port action {} is not implemented in back-end".format(action_id))
        
        backend.do_port_action(
            port_idx,
            backend.port_action_t(action_id),
            maybe_arg
        )

    #rcv_callback should be a SlowMidiCallback instance
    def create_slow_midi_input(self, name, rcv_callback):
        retval = backend.create_slow_midi_port(name.encode('ascii'), backend.Input)
        backend.set_slow_midi_port_received_callback(retval, rcv_callback.c_cb)
        return retval
    
    def create_slow_midi_output(self, name):
        return backend.create_slow_midi_port(name.encode('ascii'), backend.Output)
    
    def destroy_slow_midi_port(self, port):
        backend.destroy_slow_midi_port(port);
    
    def send_slow_midi(self, port, data):
        c_data = (c_ubyte * len(data))(*data)
        backend.send_slow_midi(port, len(data), c_data)
    
    def process_slow_midi(self):
        backend.process_slow_midi()
    
    def save_loops_to_file(self, idxs, filename, do_stop=True):
        loop_datas = [self.get_loop_data(idx, do_stop) for idx in idxs]
        # loop_datas is now a Nx2 array, reshape to 2xN
        data = np.swapaxes(loop_datas, 0, 1)
        sf.write(filename, data, backend.get_sample_rate())
    
    # Load MIDI data. Will not update the loop length, but the MIDI length
    # in samples is returned.
    @pyqtSlot(int, str, result=int)
    def load_loop_midi_from_file(self, loop_idx, filename):
        mido_file = mido.MidiFile(filename)
        mido_msgs = [msg for msg in mido_file]
        new_loop_len = int(mido_file.length * self.sample_rate) # TODO ceil?
        def backend_msg_size(mido_msg):
            return 4 + 4 + len(mido_msg.bytes()) # time + size + data
        total_bytes = reduce(lambda a, b: a+b, [backend_msg_size(msg) for msg in mido_msgs])
        total_time = 0.0
        
        c_data = (c_ubyte * total_bytes)()
        c_data_idx = 0
        for msg in mido_msgs:
            msg_bytes = msg.bytes()
            total_time += msg.time

            if (msg.is_meta and msg.type != 'end_of_track') or msg.bytes()[0] == 0xFF:
                continue # only EOT is interesting to save the length properly

            def append_uint32(val, byte_offset):
                addr = addressof(c_data) + byte_offset
                uint_ptr = cast(addr, POINTER(c_uint32))
                uint_ptr[0] = int(val)

            append_uint32(int(total_time * self.sample_rate), c_data_idx)
            c_data_idx += 4
            append_uint32(len(msg_bytes), c_data_idx)
            c_data_idx += 4
            for i in range(len(msg_bytes)):
                c_data[c_data_idx] = msg_bytes[i]
                c_data_idx += 1
        
        backend.set_loop_midi_data(
            loop_idx,
            c_data,
            c_uint(c_data_idx)
        )

        return new_loop_len
    
    @pyqtSlot(int, result=list)
    def get_loop_midi_messages(self, loop_idx):
        c_data_p = POINTER(c_ubyte)
        c_data = c_data_p()
        n_bytes = backend.get_loop_midi_data(loop_idx, byref(c_data))
        msg_start = 0
        retval = []
        while msg_start < n_bytes:
            time_samples = int(cast(addressof(c_data.contents) + msg_start, POINTER(c_uint32))[0])
            size = int(cast(addressof(c_data.contents) + msg_start + 4, POINTER(c_uint32))[0])
            msg_bytes = [int(c_data[msg_start + 8 + idx]) for idx in range(size)]
            msg = {
                'time': time_samples,
                'bytes': msg_bytes
            }
            retval.append(msg)
            msg_start += 8 + size
        backend.shoopdaloop_free(cast(c_data, c_void_p))
        return retval
    
    @pyqtSlot(int, str)
    def save_loop_midi_to_file(self, idx, filename):
        backend.freeze_midi_buffer(idx)
        msgs = self.get_loop_midi_messages(idx)
        mgr = self._channel_looper_managers[idx]
        mido_file = mido.MidiFile()
        # Use the default tempo (120) to store under, we don't know anything about our
        # time signature.
        beat_length_s = mido.bpm2tempo(120) / 1000000.0
        mido_track = mido.MidiTrack()
        mido_file.tracks.append(mido_track)
        current_tick = 0
        for msg in msgs:
            absolute_time_s = msg['time'] / self.sample_rate
            absolute_time_ticks = int(absolute_time_s / beat_length_s * mido_file.ticks_per_beat)
            d_ticks = absolute_time_ticks - current_tick
            mido_msg = mido.Message.from_bytes(msg['bytes'])
            mido_msg.time = d_ticks
            mido_track.append(mido_msg)
            current_tick += d_ticks
        mido_file.save(filename)

    @pyqtSlot(list, str)
    def load_loops_from_file(self, idxs, filename, maybe_override_length):
        try:
            np_data, samplerate = sf.read(filename, dtype='float32')
            if np_data.ndim == 1:
                np_data = [np_data]
            elif np_data.ndim == 2:
                # np_data is N_Samples elements of N_Channels numbers, swap that to
                # get per-channel arrays
                np_data = np.swapaxes(np_data, 0, 1)
            else:
                raise ValueError("Unexpected number of dimensions of sound data: {}".format(np_data.ndim))
            target_samplerate = backend.get_sample_rate()
            n_samples = len(np_data[0])
            target_n_samples = int(n_samples / float(samplerate) * target_samplerate)

            resampled = np_data
            if int(target_samplerate) != int(samplerate):
                resampled = resampy.resample(np_data, samplerate, target_samplerate)

            if maybe_override_length and maybe_override_length > len(resampled):
                last_sample = resampled[len(resampled) - 1]
                for idx in range(maybe_override_length - target_n_samples):
                    resampled.append(last_sample)
            if maybe_override_length and maybe_override_length < len(resampled):
                resampled = resampled[:maybe_override_length]
            for n,idx in enumerate(idxs):
                if n < len(resampled):
                    self.load_loop_data(idx, resampled[n])
                else:
                    print("Requested to load {} loops from a {}-channel sound file. Re-using channel {}".format(
                        len(idxs), len(resampled), len(resampled)-1
                    ))
                    self.load_loop_data(idx, resampled[len(resampled)-1])
        except Exception as e:
            print("Failed to load sound file: {}".format(format(e)))
    
    @pyqtSlot(str, str, bool)
    def save_session(self, filename, appstate_serialized, store_audio):
        def do_save():
            try:
                folder = tempfile.mkdtemp()
                session_filename = folder + '/session.json'
                tar_filename = folder + '/session.tar'
                include_in_tarball = [session_filename]

                backend.set_storage_lock(1)

                with open(session_filename, 'w') as file:
                    file.write(appstate_serialized)

                for idx in range(self.n_loops):
                    mode = self._channel_looper_managers[idx]
                    json_filename = '{}/loop_{}_mode.json'.format(folder, idx)

                    if mode.length > 0.0 and store_audio:
                        wav_filename = '{}/loop_{}_data.wav'.format(folder, idx)
                        self.save_loops_to_file([idx], wav_filename, False)
                        include_in_tarball.append(wav_filename)

                        midi_filename = '{}/loop_{}_midi.mid'.format(folder, idx)
                        self.save_loop_midi_to_file(idx, midi_filename)
                        include_in_tarball.append(midi_filename)
                    
                    with open(json_filename, 'w') as lfile:
                        lfile.write(mode.serialize_session_state())
                    include_in_tarball.append(json_filename)
                
                for idx in range(self.n_ports):
                    mode = self._channel_port_managers[idx]
                    json_filename = '{}/port_{}_mode.json'.format(folder, idx)
                    with open(json_filename, 'w') as pfile:
                        pfile.write(mode.serialize_session_state())
                    include_in_tarball.append(json_filename)

                # Now combine into a tarball
                with tarfile.open(tar_filename, 'w') as file:
                    for f in include_in_tarball:
                        file.add(f, os.path.basename(f))

                # Save as a session file
                shutil.move(tar_filename, filename)

                print("Saved session into {}".format(filename))
                self.session_saving = False
            except Exception as e:
                print("Saving session into {} failed: {}".format(filename, str(e)))
                self.session_save_failed_counter = self.session_save_failed_counter + 1
            finally:
                backend.set_storage_lock(0)
                self.session_save_counter = self.session_save_counter + 1
        
        self.session_saving = True
        thread = Thread(target=do_save)
        thread.start()
        
    @pyqtSlot(str)
    def load_session(self, filename):
        def do_load():
            try:
                for idx in range(self.n_loops):
                    self.do_backend_loops_action([idx], LoopActionType.DoClear.value, [], False)

                folder = tempfile.mkdtemp()
                session_data = None
                with tarfile.open(filename, 'r') as file:
                    file.extractall(folder)

                session_filename = folder + '/session.json'
                with open(session_filename, 'r') as file:
                    session_data = file.read()

                for idx in range(self.n_loops):
                    audio_data_filename = '{}/loop_{}_data.wav'.format(folder, idx)
                    midi_data_filename = '{}/loop_{}_midi.mid'.format(folder, idx)
                    json_filename = '{}/loop_{}_mode.json'.format(folder, idx)
                    # TODO: this assumes the # of loops is always the same.
                    if os.path.isfile(json_filename):
                        if os.path.isfile(audio_data_filename):
                            self.load_loops_from_file([idx], audio_data_filename, self._channel_looper_managers[idx].length)
                        if os.path.isfile(midi_data_filename):
                            self.load_loop_midi_from_file(idx, midi_data_filename)
                        with open(json_filename, 'r') as json:
                            self._channel_looper_managers[idx].deserialize_session_state(json.read())
                
                for idx in range(self.n_ports):
                    json_filename = '{}/port_{}_mode.json'.format(folder, idx)
                    # TODO: this assumes the # of ports is always the same.
                    if os.path.isfile(json_filename):
                        with open(json_filename, 'r') as json:
                            self._channel_port_managers[idx].deserialize_session_state(json.read())
            
                self.newSessionStateStr.emit(session_data)
                print("Loaded session from {}".format(filename))
                self.session_loading = False
            except Exception as e:
                print("Loading session from {} failed: {}".format(filename, str(e)))
                self.session_load_failed_counter = self.session_load_failed_counter + 1
            finally:
                self.session_load_counter = self.session_load_counter + 1

        self.session_loading = True
        thread = Thread(target=do_load)
        thread.start()

def c_slow_midi_rcv_callback(python_cb, port, len, data):
    p_data = [int(data[b]) for b in range(len)]
    python_cb(port, p_data)