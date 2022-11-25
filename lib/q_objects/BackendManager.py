from ctypes import *

from PyQt6.QtCore import QObject, pyqtSignal, pyqtSlot, QTimer, pyqtProperty

from .NChannelAbstractLooperManager import NChannelAbstractLooperManager

import sys
sys.path.append('../..')

import build.backend.shoopdaloop_backend as backend
from lib.StatesAndActions import *
from .PortState import PortState
from collections import OrderedDict
from third_party.pyjacklib import jacklib
from functools import partial
from threading import Thread

from .LooperState import LooperState

import time
import cProfile

import soundfile as sf
import numpy as np
import scipy as sp
import shutil
import tempfile
import tarfile

from pprint import *

class BackendManager(QObject):
    newSessionStateStr = pyqtSignal(str)
    requestLoadSession = pyqtSignal(str)
    requestSaveSession = pyqtSignal(str)

    def __init__(self,
                 port_name_pairs,
                 loops_to_ports_map,
                 loops_hard_sync_map,
                 loops_soft_sync_map,
                 max_loop_length_seconds,
                 client_name,
                 update_period_seconds,
                 parent=None):
        super(BackendManager, self).__init__(parent)

        self.update_cb = backend.UpdateCallback(self.update_cb)
        self.abort_cb = backend.AbortCallback(self.abort_cb)

        self.n_loops = len(loops_to_ports_map)
        self.n_ports = len(port_name_pairs)
        self.client_name = client_name
        self.max_loop_length_seconds = max_loop_length_seconds
        self.port_name_pairs = port_name_pairs
        self.loops_to_ports = loops_to_ports_map
        self.loops_soft_sync_map = loops_soft_sync_map
        self.loops_hard_sync_map = loops_hard_sync_map

        self.looper_states = [
            LooperState() for i in range(self.n_loops)
        ]

        self.port_states = [
            PortState() for i in range(self.n_ports)
        ]

        self.update_timer = QTimer()
        self.update_timer.setSingleShot(False)
        self.update_timer.setInterval(int(1000.0 * update_period_seconds))
        self.update_timer.timeout.connect(lambda: backend.request_update())
        self.update_timer.start()

        self._sample_rate = 0

        self._session_loading = False
        self._session_saving = False

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
    
    sampleRateChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=sampleRateChanged)
    def sample_rate(self):
        return self._sample_rate
    @sample_rate.setter
    def sample_rate(self, s):
        if self._sample_rate != s:
            self._sample_rate = s
            self.sampleRateChanged.emit(s)
    

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
        loops_soft_sync_map = (c_int * self.n_loops)()
        input_port_names = (POINTER(c_char) * self.n_ports)()
        output_port_names = (POINTER(c_char) * self.n_ports)()

        # Construct the port names and loop mappings        
        for port_idx, item in enumerate(self.port_name_pairs):
            input_port_names[port_idx] = create_string_buffer(item[0].encode('ascii'))
            output_port_names[port_idx] = create_string_buffer(item[1].encode('ascii'))
        
        for i in range(self.n_loops):
            loops_to_ports[i] = self.loops_to_ports[i]
            loops_hard_sync_map[i] = self.loops_hard_sync_map[i]
            loops_soft_sync_map[i] = self.loops_soft_sync_map[i]
        
        do_profiling = os.getenv('SHOOPDALOOP_PROFILING') != None
        features = backend.backend_features_t(backend.Profiling) if do_profiling else backend.backend_features_t(backend.Default)
        
        _jack_client = backend.initialize(
            self.n_loops,
            self.n_ports,
            self.max_loop_length_seconds,
            loops_to_ports,
            loops_hard_sync_map,
            loops_soft_sync_map,
            input_port_names,
            output_port_names,
            self.client_name.encode('ascii'),
            32768, # bit less than a second @ 48000
            self.update_cb,
            self.abort_cb,
            features
        )
        self.jack_client = cast(_jack_client, POINTER(jacklib.jack_client_t))
    
    def get_jack_input_port(self, idx):
        return cast(backend.get_port_input_handle(idx), POINTER(jacklib.jack_port_t))
    
    def remap_port_input(self, port, remapped):
        backend.remap_port_input(port, remapped)

    def reset_port_input_remap(self, port):
        backend.reset_port_input_remapping(port)

    def update_cb(
                self,
                n_loops,
                n_ports,
                sample_rate,
                loop_states,
                loop_next_states,
                loop_lengths,
                loop_positions,
                loop_volumes,
                port_volumes,
                port_passthrough_levels,
                port_latencies,
                ports_muted,
                port_inputs_muted):
        # pr = cProfile.Profile()
        # pr.enable()
        self.sample_rate = sample_rate

        for i in range(n_loops):
            m = self.looper_states[i]
            m.state = loop_states[i]
            m.next_state = loop_next_states[i]
            m.length = loop_lengths[i]
            m.pos = loop_positions[i]
            m.volume = loop_volumes[i]

        for i in range(n_ports):
            p = self.port_states[i]
            p.volume = port_volumes[i]
            p.passthrough = port_passthrough_levels[i]
            p.muted = ports_muted[i] != 0
            p.passthroughMuted = port_inputs_muted[i] != 0
            p.recordingLatency = port_latencies[i] / self.sample_rate
            if p.recordingLatency > 0.0:
                print(port_latencies[i])
        
        # # TODO port changes
        # pr.disable()
        # pr.print_stats()
        return 0

    def abort_cb(self):
        print("Backend aborted.")
        exit(1)
    
    @pyqtSlot(list, int, float, bool)
    def do_loops_action(self, loop_idxs, action_id, maybe_arg, with_soft_sync):
        for loop_idx in loop_idxs:
            if loop_idx < 0 or loop_idx >= self.n_loops:
                raise ValueError("Backend: loop idx out of range")
        
        if action_id < 0 or \
           action_id >= backend.LOOP_ACTION_MAX:
           raise ValueError("Backend: loop action {} is not implemented in back-end".format(action_id))
        
        idxs_data = (c_uint * len(loop_idxs))(*loop_idxs)
        
        backend.do_loop_action(
            idxs_data,
            len(loop_idxs),
            backend.loop_action_t(action_id),
            maybe_arg,
            with_soft_sync
        )

    @pyqtSlot(int, list)
    def load_loop_data(self, loop_idx, data):
        c_data = (c_float * len(data))(*data)
        backend.load_loop_data(loop_idx, len(data), c_data)
    
    @pyqtSlot(int, result=list)
    def get_loop_data(self, loop_idx):
        c_float_p = POINTER(c_float)
        c_data = c_float_p()
        n_floats = backend.get_loop_data(loop_idx, byref(c_data))
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
    
    def create_slow_midi_input(self, name, rcv_callback):
        retval = backend.create_slow_midi_port(name.encode('ascii'), backend.Input)
    
    def create_slow_midi_output(self, name):
        return backend.create_slow_midi_port(name.encode('ascii'), backend.Output)
    
    def destroy_slow_midi_port(self, port):
        backend.destroy_slow_midi_port(port);
    
    def send_slow_midi(self, port, data):
        c_data = (c_ubyte * len(data))(*data)
        backend.end_slow_midi(port, len(data), c_data)
    
    def set_slow_midi_port_received_callback(self, port, callback):
        cb = backend.SlowMIDIReceivedCallback(partial(c_slow_midi_rcv_callback, callback))
        backend.set_slow_midi_port_received_callback(port, cb)
    
    def process_slow_midi(self):
        backend.process_slow_midi()
    
    def save_loops_to_file(self, idxs, filename):
        loop_datas = [self.get_loop_data(idx) for idx in idxs]
        # loop_datas is now a Nx2 array, reshape to 2xN
        data = np.swapaxes(loop_datas, 0, 1)
        sf.write(filename, data, backend.get_sample_rate())
    
    @pyqtSlot(list, str)
    def load_loops_from_file(self, idxs, filename):
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
            target_n_samples = n_samples / float(samplerate) * target_samplerate
            resampled = [
                sp.signal.resample(d, int(target_n_samples)) for d in np_data
            ]
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

                with open(session_filename, 'w') as file:
                    file.write(appstate_serialized)

                for idx in range(self.n_loops):
                    state = self.looper_states[idx]
                    json_filename = '{}/loop_{}_state.json'.format(folder, idx)

                    if state.length > 0.0 and store_audio:
                        wav_filename = '{}/loop_{}_data.wav'.format(folder, idx)
                        self.save_loops_to_file([idx], wav_filename)
                        include_in_tarball.append(wav_filename)
                    
                    with open(json_filename, 'w') as lfile:
                        lfile.write(state.serialize_session_state())
                    include_in_tarball.append(json_filename)
                
                for idx in range(self.n_ports):
                    state = self.port_states[idx]
                    json_filename = '{}/port_{}_state.json'.format(folder, idx)
                    with open(json_filename, 'w') as pfile:
                        pfile.write(state.serialize_session_state())
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
        
        self.session_saving = True
        thread = Thread(target=do_save)
        thread.start()
        
    @pyqtSlot(str)
    def load_session(self, filename):
        def do_load():
            try:
                for idx in range(self.n_loops):
                    self.do_loops_action([idx], LoopActionType.DoClear.value, 0.0, False)

                folder = tempfile.mkdtemp()
                session_data = None
                with tarfile.open(filename, 'r') as file:
                    file.extractall(folder)

                session_filename = folder + '/session.json'
                with open(session_filename, 'r') as file:
                    session_data = file.read()

                for idx in range(self.n_loops):
                    data_filename = '{}/loop_{}_data.wav'.format(folder, idx)
                    json_filename = '{}/loop_{}_state.json'.format(folder, idx)
                    # TODO: this assumes the # of loops is always the same.
                    if os.path.isfile(json_filename):
                        with open(json_filename, 'r') as json:
                            self.looper_states[idx].deserialize_session_state(json.read())
                        if os.path.isfile(data_filename):
                            self.load_loops_from_file([idx], data_filename)
                
                for idx in range(self.n_ports):
                    json_filename = '{}/port_{}_state.json'.format(folder, idx)
                    # TODO: this assumes the # of ports is always the same.
                    if os.path.isfile(json_filename):
                        with open(json_filename, 'r') as json:
                            self.port_states[idx].deserialize_session_state(json.read())
            
                self.newSessionStateStr.emit(session_data)
                print("Loaded session from {}".format(filename))
                self.session_loading = False
            except Exception as e:
                print("Loading session from {} failed: {}".format(filename, str(e)))

        self.session_loading = True
        thread = Thread(target=do_load)
        thread.start()

def c_slow_midi_rcv_callback(python_cb, port, len, data):
    p_data = [int(data[b]) for b in range(len)]
    python_cb(port, p_data)