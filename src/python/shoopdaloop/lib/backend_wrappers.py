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

class PyFXChainType(Enum):
    Carla_Rack = bindings.Carla_Rack
    Carla_Patchbay = bindings.Carla_Patchbay
    Carla_Patchbay_16x = bindings.Carla_Patchbay_16x
    Test2x2x1 = bindings.Test2x2x1

DontWaitForSync = -1
DontAlignToSyncImmediately = -1


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

def open_driver_decoupled_midi_port(audio_driver, name_hint : str, direction : int) -> 'BackendDecoupledMidiPort':
    obj = shoop_py_backend.open_driver_decoupled_midi_port(
        audio_driver,
        name_hint,
        direction
    )
    port = BackendDecoupledMidiPort(obj)
    return port

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
