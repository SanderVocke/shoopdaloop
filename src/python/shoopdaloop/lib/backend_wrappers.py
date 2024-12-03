import sys
import os
from typing import *
from ctypes import *
from enum import Enum
from dataclasses import dataclass
import typing
import copy
import threading
import time
import sys
import importlib
import inspect
import ctypes
import traceback
import numpy

from shoop_app_info import shoop_dynlib_dir
import shoop_py_backend
from shoopdaloop.lib.init_dynlibs import init_dynlibs
from shoop_py_backend import AudioChannel
init_dynlibs()

# On Windows, shoopdaloop.dll depends on shared libraries in the same folder.
# this folder needs to be added to the PATH before loading
os.environ['PATH'] = os.environ['PATH'] + os.pathsep + shoop_dynlib_dir

# Ensure we can find all dependency back-end dynamic libraries
sys.path.append(shoop_dynlib_dir)

bindings = importlib.import_module('libshoopdaloop_backend_bindings')

intmax = 2**31-1
intmin = -intmax - 1
def to_int(val):
    v = int(val)
    return max(min(v, intmax), intmin)

class PortDirection(Enum):
    Input = bindings.ShoopPortDirection_Input
    Output = bindings.ShoopPortDirection_Output
    Any = bindings.ShoopPortDirection_Any

class PortDataType(Enum):
    Audio = bindings.ShoopPortDataType_Audio
    Midi = bindings.ShoopPortDataType_Midi
    Any = bindings.ShoopPortDataType_Any

class PortConnectability(Enum):
    Internal = bindings.ShoopPortConnectability_Internal
    External = bindings.ShoopPortConnectability_External
class BackendResult(Enum):
    Success = bindings.Success
    Failure = bindings.Failure

class PyLoopMode(Enum):
    Unknown = int(shoop_py_backend.LoopMode.Unknown)
    Stopped = int(shoop_py_backend.LoopMode.Stopped)
    Playing = int(shoop_py_backend.LoopMode.Playing)
    Recording = int(shoop_py_backend.LoopMode.Recording)
    Replacing = int(shoop_py_backend.LoopMode.Replacing)
    PlayingDryThroughWet = int(shoop_py_backend.LoopMode.PlayingDryThroughWet)
    RecordingDryIntoWet = int(shoop_py_backend.LoopMode.RecordingDryIntoWet)

class PyChannelMode(Enum):
    Disabled = bindings.ChannelMode_Disabled
    Direct = bindings.ChannelMode_Direct
    Dry = bindings.ChannelMode_Dry
    Wet = bindings.ChannelMode_Wet

class PyAudioDriverType(Enum):
    Jack = bindings.Jack
    JackTest = bindings.JackTest
    Dummy = bindings.Dummy

class FXChainType(Enum):
    Carla_Rack = bindings.Carla_Rack
    Carla_Patchbay = bindings.Carla_Patchbay
    Carla_Patchbay_16x = bindings.Carla_Patchbay_16x
    Test2x2x1 = bindings.Test2x2x1

DontWaitForSync = -1
DontAlignToSyncImmediately = -1

@dataclass
class FXChainState:
    visible : bool
    ready : bool
    active : bool

    def __init__(self, backend_state : "bindings.shoop_fx_chain_state_info_t" = None):
        if backend_state:
            self.visible = backend_state.visible
            self.active = backend_state.active
            self.ready = backend_state.ready
        else:
            self.visible = False
            self.active = False
            self.ready = False

@dataclass
class AudioPortState:
    input_peak: float
    output_peak: float
    gain: float
    muted: bool
    passthrough_muted: bool
    name: str
    ringbuffer_n_samples : int

    def __init__(self, backend_state : 'bindings.shoop_audio_port_state_info_t' = None):
        if backend_state:
            self.input_peak = float(backend_state.input_peak)
            self.output_peak = float(backend_state.output_peak)
            self.gain = float(backend_state.gain)
            self.muted = bool(backend_state.muted)
            self.passthrough_muted = bool(backend_state.passthrough_muted)
            self.name = str(backend_state.name)
            self.ringbuffer_n_samples = int(backend_state.ringbuffer_n_samples)
        else:
            self.input_peak = 0.0
            self.output_peak = 0.0
            self.gain = 0.0
            self.muted = False
            self.passthrough_muted = False
            self.name = '(unknown)'
            self.ringbuffer_n_samples = 0

@dataclass
class MidiPortState:
    n_input_events: int
    n_input_notes_active : int
    n_output_events: int
    n_output_notes_active: int
    muted: bool
    passthrough_muted: bool
    name: str
    ringbuffer_n_samples : int

    def __init__(self, backend_state : 'bindings.shoop_audio_port_state_info_t' = None):
        if backend_state:
            self.n_input_events = to_int(backend_state.n_input_events)
            self.n_input_notes_active = to_int(backend_state.n_input_notes_active)
            self.n_output_events = to_int(backend_state.n_output_events)
            self.n_output_notes_active = to_int(backend_state.n_output_notes_active)
            self.muted = bool(backend_state.muted)
            self.passthrough_muted = bool(backend_state.passthrough_muted)
            self.name = str(backend_state.name)
            self.ringbuffer_n_samples = int(backend_state.ringbuffer_n_samples)
        else:
            self.n_input_events = 0
            self.n_input_notes_active = 0
            self.n_output_events = 0
            self.n_output_notes_active = 0
            self.muted = False
            self.passthrough_muted = False
            self.name = '(unknown)'
            self.ringbuffer_n_samples = 0

@dataclass
class MidiEvent:
    time: int
    size: int
    data: List[int]

    def __init__(self, backend_event : 'bindings.shoop_midi_event_t'):
        if backend_event:
            self.time = to_int(backend_event.time)
            self.size = to_int(backend_event.size)
            self.data = [int(backend_event.data[i]) for i in range(backend_event.size)]
        else:
            self.time = 0
            self.size = 0
            self.data = []

@dataclass
class ExternalPortDescriptor:
    name : str
    direction : PortDirection
    data_type : PortDataType

    def __init__(self, backend_obj : 'bindings.shoop_external_port_descriptor_t'):
        if backend_obj:
            self.name = str(backend_obj.name)
            self.direction = backend_obj.direction
            self.data_type = backend_obj.data_type
        else:
            self.name = None
            self.direction = None
            self.data_type = None

    def to_basic_dict(self):
        return {
            'name': self.name,
            'direction': int(self.direction),
            'data_type': int(self.data_type)
        }

def deref_ptr(backend_ptr):
    if not backend_ptr:
        return None
    return backend_ptr[0]

def parse_connections_state(backend_state : 'bindings.port_connections_state_info_t'):
    if not backend_state:
        return dict()
    rval = dict()
    for i in range(backend_state.n_ports):
        rval[str(backend_state.ports[i].name)] = bool(backend_state.ports[i].connected)
    return rval

def backend_midi_message_to_dict(backend_msg: 'bindings.shoop_midi_event_t'):
    r = {
        'time': 0,
        'data': []
    }
    if backend_msg:
        r['time'] = backend_msg.time
        r['data'] = [int(backend_msg.data[i]) for i in range(backend_msg.size)]
    return r

def midi_message_dict_to_backend(msg):
    m = bindings.alloc_midi_event(len(msg['data']))
    m[0].time = msg['time']
    m[0].size = len(msg['data'])
    for i in range(len(msg['data'])):
        m[0].data[i] = msg['data'][i]
    return m

class BackendAudioPort:
    def __init__(self,
                 obj):
        self._obj = obj
        self._input_connectability = bindings.get_audio_port_input_connectability(self.get_backend_obj())
        self._output_connectability = bindings.get_audio_port_output_connectability(self.get_backend_obj())
        self._name = '(unknown)'
        
    def get_backend_obj(self):
        addr = self._obj.unsafe_backend_ptr()
        return cast(c_void_p(addr), POINTER(bindings.shoopdaloop_audio_port_t))
    
    def available(self):
        return self.get_backend_obj()

    def input_connectability(self) -> Type['PortConnectability']:
        return self._input_connectability

    def output_connectability(self) -> Type['PortConnectability']:
        return self._output_connectability

    def name(self):
        return self._name

    def get_state(self):
        if self.available():
            state = bindings.get_audio_port_state(self.get_backend_obj())
            rval = AudioPortState(deref_ptr(state))
            self._name = rval.name
            if state:
                bindings.destroy_audio_port_state_info(state)
            return rval
        else:
            return AudioPortState()

    def set_gain(self, gain):
        if self.available():
            bindings.set_audio_port_gain(self.get_backend_obj(), gain)

    def set_muted(self, muted):
        if self.available():
            bindings.set_audio_port_muted(self.get_backend_obj(), (1 if muted else 0))

    def set_passthrough_muted(self, muted):
        if self.available():
            bindings.set_audio_port_passthroughMuted(self.get_backend_obj(), (1 if muted else 0))

    def connect_internal(self, other):
            if self.available():
                bindings.connect_audio_port_internal(self.get_backend_obj(), other.get_backend_obj())

    def dummy_queue_data(self, data):
        if self.available():
            data_type = c_float * len(data)
            arr = data_type()
            for i in range(len(data)):
                arr[i] = data[i]
            bindings.dummy_audio_port_queue_data(self.get_backend_obj(), len(data), arr)

    def dummy_dequeue_data(self, n_samples):
        if self.available():
            data_type = c_float * n_samples
            arr = data_type()
            bindings.dummy_audio_port_dequeue_data(self.get_backend_obj(), n_samples, arr)
            return [float(arr[i]) for i in range(n_samples)]
        return [0.0 for i in range(n_samples)]

    def dummy_request_data(self, n_samples):
        if self.available():
            bindings.dummy_audio_port_request_data(self.get_backend_obj(), n_samples)

    def get_connections_state(self):
        if self.available():
            state = bindings.get_audio_port_connections_state(self.get_backend_obj())
            rval = parse_connections_state(deref_ptr(state))
            if state:
                bindings.destroy_port_connections_state(state)
            return rval
        else:
            return dict()

    def connect_external_port(self, name):
        if self.available():
            bindings.connect_audio_port_external(self.get_backend_obj(), name.encode('ascii'))

    def disconnect_external_port(self, name):
        if self.available():
            bindings.disconnect_audio_port_external(self.get_backend_obj(), name.encode('ascii'))

    def set_ringbuffer_n_samples(self, n):
        if self.available():
            bindings.set_audio_port_ringbuffer_n_samples(self.get_backend_obj(), n)

class BackendDecoupledMidiPort:
    def __init__(self, obj):
        self._obj = obj

    def available(self):
        return self.get_backend_obj()
    
    def get_backend_obj(self):
        addr = self._obj.unsafe_backend_ptr()
        return cast(c_void_p(addr), POINTER(bindings.shoopdaloop_decoupled_midi_port_t))

    def maybe_next_message(self):
        if self.available():
            r = bindings.maybe_next_message(self.get_backend_obj())
            if r:
                rval = MidiEvent(r[0])
                bindings.destroy_midi_event(r)
                return rval
        return None

    def name(self):
        if self.available():
            return bindings.get_decoupled_midi_port_name(self.get_backend_obj()).decode('ascii')
        return '(unknown)'

    def send_midi(self, msg):
        if self.available():
            data_type = c_ubyte * len(msg)
            arr = data_type()
            for i in range(len(msg)):
                arr[i] = msg[i]
                bindings.send_decoupled_midi(self._c_handle, len(msg), arr)

    def get_connections_state(self):
        if self.available():
            state = bindings.get_decoupled_midi_port_connections_state(self._c_handle)
            rval = parse_connections_state(deref_ptr(state))
            if state:
                bindings.destroy_port_connections_state(state)
            return rval
        return dict()

    def connect_external_port(self, name):
        if self.available():
            bindings.connect_external_decoupled_midi_port(self._c_handle, name.encode('ascii'))

    def disconnect_external_port(self, name):
        if self.available():
            bindings.disconnect_external_decoupled_midi_port(self._c_handle, name.encode('ascii'))

class BackendMidiPort:
    def __init__(self,
                 obj):
        self._obj = obj
        self._name = '(unknown)'
        self._input_connectability = bindings.get_midi_port_input_connectability(self.get_backend_obj())
        self._output_connectability = bindings.get_midi_port_output_connectability(self.get_backend_obj())

    def get_backend_obj(self):
        addr = self._obj.unsafe_backend_ptr()
        return cast(c_void_p(addr), POINTER(bindings.shoopdaloop_midi_port_t))

    def available(self):
        return self.get_backend_obj()

    def input_connectability(self) -> Type['PortConnectability']:
        return self._input_connectability

    def output_connectability(self) -> Type['PortConnectability']:
        return self._output_connectability

    def name(self):
        return self._name

    def get_state(self):
        if self.available():
            state = bindings.get_midi_port_state(self.get_backend_obj())
            rval = MidiPortState(deref_ptr(state))
            self._name = rval.name
            if state:
                bindings.destroy_midi_port_state_info(state)
            return rval
        return MidiPortState()

    def set_muted(self, muted):
        if self.available():
            bindings.set_midi_port_muted(self.get_backend_obj(), (1 if muted else 0))

    def set_passthrough_muted(self, muted):
        if self.available():
            bindings.set_midi_port_passthroughMuted(self.get_backend_obj(), (1 if muted else 0))

    def connect_internal(self, other):
        if self.available():
            bindings.connect_midi_port_internal(self.get_backend_obj(), other.get_backend_obj())

    def dummy_clear_queues(self):
        if self.available():
            bindings.dummy_midi_port_clear_queues(self.get_backend_obj())

    def dummy_queue_msg(self, msg):
        if self.available():
            return self.dummy_queue_msgs([msg])

    def dummy_queue_msgs(self, msgs):
        if self.available():
            d = bindings.alloc_midi_sequence(len(msgs))
            if not d:
                return
            d[0].length_samples = msgs[len(msgs)-1]['time'] + 1
            for idx, m in enumerate(msgs):
                d[0].events[idx] = midi_message_dict_to_backend(m)
            bindings.dummy_midi_port_queue_data(self.get_backend_obj(), d)
            bindings.destroy_midi_sequence(d)

    def dummy_dequeue_data(self):
        if self.available():
            r = bindings.dummy_midi_port_dequeue_data(self.get_backend_obj())
            if r:
                msgs = [backend_midi_message_to_dict(r[0].events[i][0]) for i in range(r[0].n_events)]
                bindings.destroy_midi_sequence(r)
                return msgs
        return []

    def dummy_request_data(self, n_frames):
        if self.available():
            bindings.dummy_midi_port_request_data(self.get_backend_obj(), n_frames)

    def get_connections_state(self):
        if self.available():
            state = bindings.get_midi_port_connections_state(self.get_backend_obj())
            rval = parse_connections_state(deref_ptr(state))
            if state:
                bindings.destroy_port_connections_state(state)
            return rval
        return dict()

    def connect_external_port(self, name):
        if self.available():
            bindings.connect_midi_port_external(self.get_backend_obj(), name.encode('ascii'))

    def disconnect_external_port(self, name):
        if self.available():
            bindings.disconnect_midi_port_external(self.get_backend_obj(), name.encode('ascii'))

    def set_ringbuffer_n_samples(self, n):
        if self.available():
            bindings.set_midi_port_ringbuffer_n_samples(self.get_backend_obj(), n)

class BackendFXChain:
    def __init__(self, c_handle : "POINTER(bindings.shoopdaloop_fx_chain_t)", chain_type: FXChainType,
                 backend : 'BackendSession'):
        self._type = chain_type
        self._c_handle = c_handle
        self._backend = backend

    def available(self):
        return self._c_handle and self._backend and self._backend.active()

    def chain_type(self) -> Type['FXChainType']:
        return self._type

    def c_handle(self):
        return self._c_handle

    def set_visible(self, visible):
        if self.available():
            bindings.fx_chain_set_ui_visible(self._c_handle, int(visible))

    def set_active(self, active):
        if self.available():
            bindings.set_fx_chain_active(self._c_handle, int(active))

    def get_state(self):
        if self.available():
            state = bindings.get_fx_chain_state(self._c_handle)
            rval = FXChainState(deref_ptr(state))
            if state:
                bindings.destroy_fx_chain_state(state)
            return rval
        return FXChainState()

    def get_state_str(self):
        if self.available():
            state = bindings.get_fx_chain_internal_state(self._c_handle)
            rval = state.decode('ascii')
            # TODO destroy_string(state)
            return rval
        return ''

    def restore_state(self, state_str):
        if self.available():
            bindings.restore_fx_chain_internal_state(self._c_handle, c_char_p(bytes(state_str, 'ascii')))
            
    def get_audio_input_port(self, idx : int):
        if self.active():
            ptr = bindings.fx_chain_audio_input_port(self.c_handle(), idx)
            port = shoop_py_backend.unsafe_audio_port_from_raw_ptr(
                ctypes.cast(ptr, ctypes.c_void_p).value
            )
            return BackendAudioPort(port)
        return None

    def get_audio_output_port(self, idx : int):
        if self.active():
            ptr = bindings.fx_chain_audio_output_port(self.c_handle(), idx)
            port = shoop_py_backend.unsafe_audio_port_from_raw_ptr(
                ctypes.cast(ptr, ctypes.c_void_p).value
            )
            return BackendAudioPort(port)
        return None

    def get_midi_input_port(self, idx : int):
        if self.active():
            ptr = bindings.fx_chain_midi_input_port(self.c_handle(), idx)
            port = shoop_py_backend.unsafe_midi_port_from_raw_ptr(
                ctypes.cast(ptr, ctypes.c_void_p).value
            )
            return BackendMidiPort(port)
        return None

def open_driver_audio_port(backend_session, audio_driver, name_hint : str, direction : int, min_n_ringbuffer_samples : int) -> 'BackendAudioPort':
    obj = shoop_py_backend.open_driver_audio_port(
            backend_session,
            audio_driver,
            name_hint,
            direction,
            min_n_ringbuffer_samples)
    return BackendAudioPort(obj)
   
def open_driver_decoupled_midi_port(audio_driver, name_hint : str, direction : int) -> 'BackendDecoupledMidiPort':
    obj = shoop_py_backend.open_driver_decoupled_midi_port(
        audio_driver,
        name_hint,
        direction
    )
    port = BackendDecoupledMidiPort(obj)
    return port

def open_driver_midi_port(backend_session, audio_driver, name_hint : str, direction : int, min_n_ringbuffer_samples : int) -> 'BackendMidiPort':
    obj = shoop_py_backend.open_driver_midi_port(
            backend_session,
            audio_driver,
            name_hint,
            direction,
            min_n_ringbuffer_samples)
    return BackendMidiPort(obj)

def resample_audio(audio, target_n_frames):
    n_channels = audio.shape[1] # inner
    n_frames = audio.shape[0] # outer
    if not n_channels or not n_frames:
        return audio
    
    audio_obj = shoop_py_backend.MultichannelAudio(n_channels, n_frames)
    resampled_obj = audio_obj.resample(target_n_frames)
    result = numpy.zeros_like(audio, shape=[target_n_frames, n_channels])
    for chan in range(n_channels):
        for frame in range(target_n_frames):
            result[frame, chan] = resampled_obj.at(frame, chan)
    return result
