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
from .directories import *
import importlib
import inspect
import ctypes
import traceback
import numpy

from shoopdaloop.lib.init_dynlibs import init_dynlibs
init_dynlibs()

all_active_backends = set()
all_active_drivers = set()

# On Windows, shoopdaloop.dll depends on shared libraries in the same folder.
# this folder needs to be added to the PATH before loading
os.environ['PATH'] = os.environ['PATH'] + os.pathsep + installation_dir()

# Ensure we can find all dependency back-end dynamic libraries
sys.path.append(dynlibs_dir())

bindings = importlib.import_module('libshoopdaloop_bindings')

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
class LoopMode(Enum):
    Unknown = bindings.LoopMode_Unknown
    Stopped = bindings.LoopMode_Stopped
    Playing = bindings.LoopMode_Playing
    Recording = bindings.LoopMode_Recording
    Replacing = bindings.LoopMode_Replacing
    PlayingDryThroughWet = bindings.LoopMode_PlayingDryThroughWet
    RecordingDryIntoWet = bindings.LoopMode_RecordingDryIntoWet

class ChannelMode(Enum):
    Disabled = bindings.ChannelMode_Disabled
    Direct = bindings.ChannelMode_Direct
    Dry = bindings.ChannelMode_Dry
    Wet = bindings.ChannelMode_Wet

class AudioDriverType(Enum):
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
class LoopAudioChannelState:
    mode : Type[ChannelMode]
    output_peak : float
    gain : float
    length : int
    start_offset : int
    data_dirty : bool
    played_back_sample : Any
    n_preplay_samples : int

    def __init__(self, backend_state : 'bindings.loop_audio_channel_state_t' = None):
        if backend_state:
            self.output_peak = backend_state.output_peak
            self.gain = backend_state.gain
            self.mode = ChannelMode(backend_state.mode)
            self.length = to_int(backend_state.length)
            self.start_offset = to_int(backend_state.start_offset)
            self.data_dirty = bool(backend_state.data_dirty)
            self.played_back_sample = (to_int(backend_state.played_back_sample) if backend_state.played_back_sample >= 0 else None)
            self.n_preplay_samples = to_int(backend_state.n_preplay_samples)
        else:
            self.output_peak = 0.0
            self.gain = 0.0
            self.mode = ChannelMode.Disabled
            self.length = 0
            self.start_offset = 0
            self.data_dirty = False
            self.played_back_sample = None
            self.n_preplay_samples = 0

@dataclass
class LoopMidiChannelState:
    mode: Type[ChannelMode]
    n_events_triggered : int
    n_notes_active : int
    length: int
    start_offset: int
    data_dirty : bool
    played_back_sample : Any
    n_preplay_samples : int

    def __init__(self, backend_state : 'bindings.loop_midi_channel_state_t' = None):
        if backend_state:
            self.n_events_triggered = to_int(backend_state.n_events_triggered)
            self.n_notes_active = to_int(backend_state.n_notes_active)
            self.mode = ChannelMode(backend_state.mode)
            self.length = to_int(backend_state.length)
            self.start_offset = to_int(backend_state.start_offset)
            self.data_dirty = bool(backend_state.data_dirty)
            self.played_back_sample = (to_int(backend_state.played_back_sample) if backend_state.played_back_sample >= 0 else None)
            self.n_preplay_samples = to_int(backend_state.n_preplay_samples)
        else:
            self.n_events_triggered = 0
            self.n_notes_active = 0
            self.mode = ChannelMode.Disabled
            self.length = 0
            self.start_offset = 0
            self.data_dirty = False
            self.played_back_sample = None
            self.n_preplay_samples = 0

@dataclass
class LoopState:
    length: int
    position: int
    mode: Type[LoopMode]
    maybe_next_mode: typing.Union[Type[LoopMode], None]
    maybe_next_delay: typing.Union[int, None]

    def __init__(self, backend_loop_state : 'bindings.loop_state_t' = None):
        if backend_loop_state:
            self.length = to_int(backend_loop_state.length)
            self.position = to_int(backend_loop_state.position)
            self.mode = backend_loop_state.mode
            self.maybe_next_mode =  (None if backend_loop_state.maybe_next_mode == bindings.LOOP_MODE_INVALID else backend_loop_state.maybe_next_mode)
            self.maybe_next_delay = (None if backend_loop_state.maybe_next_mode == bindings.LOOP_MODE_INVALID else backend_loop_state.maybe_next_mode_delay)
        else:
            self.length = 0
            self.position = 0
            self.mode = LoopMode.Unknown
            self.maybe_next_mode = None
            self.maybe_next_delay = None
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
class BackendSessionState:
    audio_driver_handle : Any

    def __init__(self, backend_state : 'bindings.shoop_backend_session_state_info_t' = None):
        if backend_state:
            self.audio_driver_handle = backend_state.audio_driver
        else:
            self.audio_driver_handle = None

@dataclass
class ProfilingReportItem:
    key : str
    n_samples : float
    worst : float
    most_recent : float
    average : float

    def __init__(self, backend_obj : 'bindings.shoop_profiling_report_item_t'):
        if backend_obj:
            self.key = str(backend_obj.key)
            self.n_samples = float(backend_obj.n_samples)
            self.worst = float(backend_obj.worst)
            self.most_recent = float(backend_obj.most_recent)
            self.average = float(backend_obj.average)
        else:
            self.key = 'unknown'
            self.n_samples = 0.0
            self.worst = 0.0
            self.most_recent = 0.0
            self.average = 0.0

@dataclass
class ProfilingReport:
    items : List[ProfilingReportItem]

    def __init__(self, backend_obj : 'bindings.shoop_profiling_report_t'):
        if backend_obj:
            self.items = [ProfilingReportItem(backend_obj.items[i]) for i in range(backend_obj.n_items)] if backend_obj else []
        else:
            self.items = []

@dataclass
class AudioDriverState:
    dsp_load : float
    xruns : int
    maybe_driver_handle : Any
    maybe_instance_name : str
    sample_rate : int
    buffer_size : int
    active : bool
    last_processed : int
    
    def __init__(self, backend_obj : 'bindings.shoop_audio_driver_state_t'):
        if backend_obj:
            self.dsp_load = float(backend_obj.dsp_load_percent)
            self.xruns = to_int(backend_obj.xruns_since_last)
            self.maybe_driver_handle = backend_obj.maybe_driver_handle
            self.maybe_instance_name = str(backend_obj.maybe_instance_name)
            self.sample_rate = to_int(backend_obj.sample_rate)
            self.buffer_size = to_int(backend_obj.buffer_size)
            self.active = bool(backend_obj.active)
            self.last_processed = to_int(backend_obj.last_processed)
        else:
            self.dsp_load = 0.0
            self.xruns = 0
            self.maybe_driver_handle = None
            self.maybe_instance_name = 'unknown'
            self.sample_rate = 48000
            self.buffer_size = 1024
            self.active = False
            self.last_processed = 1

@dataclass
class JackAudioDriverSettings:
    client_name_hint : str
    maybe_server_name : str

    def to_backend(self):
        rval = bindings.shoop_jack_audio_driver_settings_t()
        rval.client_name_hint = bindings.String(self.client_name_hint.encode('ascii'))
        rval.maybe_server_name = bindings.String(self.maybe_server_name.encode('ascii'))
        return rval
    
@dataclass
class DummyAudioDriverSettings:
    client_name : str
    sample_rate : int
    buffer_size : int

    def to_backend(self):
        rval = bindings.shoop_dummy_audio_driver_settings_t()
        rval.client_name = bindings.String(self.client_name.encode('ascii'))
        rval.sample_rate = self.sample_rate
        rval.buffer_size = self.buffer_size
        return rval
    
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

class ShoopChannelAudioData:
    def __init__(self, data=None):
        if data == None:
            self.ctypes_array = None
            self.raw_data_ptr = None
            self.backend_obj = None
            self.np_array = numpy.empty(0)
            return      
        if not isinstance(data, ctypes.POINTER(bindings.shoop_audio_channel_data_t)):
            raise ValueError("array must be a backend bindings audio data object, is {}".format(type(data)))
        if not isinstance(data[0].data, ctypes.POINTER(ctypes.c_float)):
            raise ValueError("array data must be a ctypes float pointer, is {}".format(type(data[0].data)))
        self.backend_obj = data
        self.np_array = numpy.ctypeslib.as_array(self.backend_obj[0].data, shape=(self.backend_obj[0].n_samples,))
    
    def __len__(self):
        return len(self.np_array)
    
    def __del__(self):
        self.np_array = None
        bindings.destroy_audio_channel_data(self.backend_obj)

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

class BackendLoopAudioChannel:
    def __init__(self, loop, c_handle : 'POINTER(bindings.shoopdaloop_loop_audio_channel)', backend: 'Backend'):
        self.loop_shoop_c_handle = loop.c_handle()
        self.shoop_c_handle = c_handle
        self._backend = backend
    
    def available(self):
        return self.shoop_c_handle and self.loop_shoop_c_handle and self._backend and self._backend.active()
    
    def connect_input(self, port : 'BackendAudioPort'):
        if self.available():
                bindings.connect_audio_input(self.shoop_c_handle, port.c_handle())
    
    def connect_output(self, port: 'BackendAudioPort'):
        if self.available():
                bindings.connect_audio_output(self.shoop_c_handle, port.c_handle())
    
    def disconnect(self, port : 'BackendAudioPort'):
        if self.available():
            if port.direction() == PortDirection.Input:
                bindings.disconnect_audio_input(self.shoop_c_handle, port.c_handle())
            else:
                bindings.disconnect_audio_output(self.shoop_c_handle, port.c_handle())
    
    def load_data(self, data):
        if self.available():
            backend_data = bindings.alloc_audio_channel_data(len(data))
            if not backend_data:
                return
            for i in range(len(data)):
                backend_data[0].data[i] = data[i]
            bindings.load_audio_channel_data(self.shoop_c_handle, backend_data)
            bindings.destroy_audio_channel_data(backend_data)
    
    def get_data(self) -> List[float]:
        if self.available():
            import time
            start = time.time()
            rval = ShoopChannelAudioData(bindings.get_audio_channel_data(self.shoop_c_handle))
            got = time.time()
            return rval
        return ShoopChannelAudioData()
    
    def get_state(self):
        if self.available():
            state = bindings.get_audio_channel_state(self.shoop_c_handle)
            rval = LoopAudioChannelState(deref_ptr(state))
            if state:
                bindings.destroy_audio_channel_state_info(state)
            return rval
        return LoopAudioChannelState()
    
    def set_gain(self, gain):
        if self.available():
            bindings.set_audio_channel_gain(self.shoop_c_handle, gain)
    
    def set_mode(self, mode : Type['ChannelMode']):
        if self.available():
            bindings.set_audio_channel_mode(self.shoop_c_handle, mode.value)
    
    def set_start_offset(self, offset):
        if self.available():
            bindings.set_audio_channel_start_offset(self.shoop_c_handle, offset)
    
    def set_n_preplay_samples(self, n):
        if self.available():
            bindings.set_audio_channel_n_preplay_samples(self.shoop_c_handle, n)

    def destroy(self):
        if self.available():
            bindings.destroy_audio_channel(self.shoop_c_handle)
            self.shoop_c_handle = None
    
    def clear_data_dirty(self):
        if self.available():
            bindings.clear_audio_channel_data_dirty(self.shoop_c_handle)

    def clear(self, length=0):
        if self.available():
            bindings.clear_audio_channel(self.shoop_c_handle, length)

    def __del__(self):
        if self.available():
            self.destroy()

class BackendLoopMidiChannel:
    def __init__(self, loop : 'BackendLoop', c_handle : 'POINTER(bindings.shoopdaloop_loop_midi_channel_t)',
                 backend: 'BackendSession'):
        self.loop_shoop_c_handle = loop.c_handle()
        self.shoop_c_handle = c_handle
        self._backend = backend
    
    def available(self):
        return self.shoop_c_handle and self.loop_shoop_c_handle and self._backend and self._backend.active()
    
    def get_all_midi_data(self):
        if self.available():
            r = bindings.get_midi_channel_data(self.shoop_c_handle)
            if r:
                msgs = [backend_midi_message_to_dict(r[0].events[i][0]) for i in range(r[0].n_events)]
                bindings.destroy_midi_sequence(r)
                return msgs
        return []

    def get_recorded_midi_msgs(self):
        return [m for m in self.get_all_midi_data() if m['time'] >= 0.0]

    def get_state_midi_msgs(self):
        return [m for m in self.get_all_midi_data() if m['time'] < 0.0]
    
    def load_all_midi_data(self, msgs):
        if self.available():
            d = bindings.alloc_midi_sequence(len(msgs))
            if d:
                d[0].length_samples = msgs[len(msgs)-1]['time'] + 1
                for idx, m in enumerate(msgs):
                    d[0].events[idx] = midi_message_dict_to_backend(m)
                bindings.load_midi_channel_data(self.shoop_c_handle, d)
                bindings.destroy_midi_sequence(d)
    
    def connect_input(self, port : 'BackendMidiPort'):
        if self.available():
                bindings.connect_midi_input(self.shoop_c_handle, port.c_handle())
    
    def connect_output(self, port: 'BackendMidiPort'):
        if self.available():
                bindings.connect_midi_output(self.shoop_c_handle, port.c_handle())
        
    def disconnect(self, port : 'BackendMidiPort'):
        if self.available():
            if port.direction() == PortDirection.Input:
                bindings.disconnect_midi_input(self.shoop_c_handle, port.c_handle())
            else:
                bindings.disconnect_midi_output(self.shoop_c_handle, port.c_handle())
    
    def get_state(self):
        if self.available():
            state = bindings.get_midi_channel_state(self.shoop_c_handle)
            rval = LoopMidiChannelState(deref_ptr(state))
            if state:
                bindings.destroy_midi_channel_state_info(state)
            return rval
        return LoopMidiChannelState()
    
    def set_mode(self, mode : Type['ChannelMode']):
        if self.available():
            bindings.set_midi_channel_mode(self.shoop_c_handle, mode.value)
    
    def set_start_offset(self, offset):
        if self.available():
            bindings.set_midi_channel_start_offset(self.shoop_c_handle, offset)
    
    def set_n_preplay_samples(self, n):
        if self.available():
            bindings.set_midi_channel_n_preplay_samples(self.shoop_c_handle, n)
    
    def destroy(self):
        if self.available():
            bindings.destroy_midi_channel(self.shoop_c_handle)
            self.shoop_c_handle = None
    
    def clear_data_dirty(self):
        if self.available():
            bindings.clear_midi_channel_data_dirty(self.shoop_c_handle)
    
    def clear(self):
        if self.available():
            bindings.clear_midi_channel(self.shoop_c_handle)

    def reset_state_tracking(self):
        if self.available():
            bindings.reset_midi_channel_state_tracking(self.shoop_c_handle)

    def __del__(self):
        if self.available():
            self.destroy()

class BackendLoop:
    def __init__(self, c_handle : 'POINTER(bindings.shoopdaloop_loop_t)', backend: 'BackendSession'):
        self.shoop_c_handle = c_handle
        self._backend = backend
    
    def available(self):
        return self.shoop_c_handle and self._backend and self._backend.active()
    
    def c_handle(self):
        return self.shoop_c_handle
    
    def add_audio_channel(self, mode : Type['ChannelMode']) -> 'BackendLoopAudioChannel':
        if self.available():
            rval = BackendLoopAudioChannel(self, bindings.add_audio_channel(self.c_handle(), mode.value), self._backend)
            return rval
        return None
    
    def add_midi_channel(self, mode : Type['ChannelMode']) -> 'BackendLoopMidiChannel':
        if self.available():
            rval = BackendLoopMidiChannel(self, bindings.add_midi_channel(self.c_handle(), mode.value), self._backend)
            return rval
        return None

    def transition(self,
                   to_state : Type['LoopMode'],
                   maybe_cycles_delay : int,
                   maybe_to_sync_at_cycle : int):
        if self.available():
            bindings.loop_transition(self.shoop_c_handle,
                                    to_state.value,
                                    maybe_cycles_delay,
                                    maybe_to_sync_at_cycle)
    
    # Static version for multiple loops
    def transition_multiple(loops, to_state : Type['LoopMode'],
                   maybe_cycles_delay : int,
                   maybe_to_sync_at_cycle : int):
        if len(loops) == 0:
            return
        backend = loops[0]._backend
        if backend and backend.active():
            HandleType = POINTER(bindings.shoopdaloop_loop_t)
            handles = (HandleType * len(loops))()
            for idx,l in enumerate(loops):
                handles[idx] = l.c_handle()
            bindings.loops_transition(len(loops),
                                    handles,
                                    to_state.value,
                                    maybe_cycles_delay,
                                    maybe_to_sync_at_cycle)
            del handles
    
    def get_state(self):
        if self.available():
            state = bindings.get_loop_state(self.shoop_c_handle)
            rval = LoopState(deref_ptr(state))
            if state:
                bindings.destroy_loop_state_info(state)
            return rval
        return LoopState()
    
    def set_length(self, length):
        if self.available():
            bindings.set_loop_length(self.shoop_c_handle, length)
    
    def set_position(self, position):
        if self.available():
            bindings.set_loop_position(self.shoop_c_handle, position)
    
    def clear(self, length):
        if self.available():
            bindings.clear_loop(self.shoop_c_handle, length)
    
    def set_sync_source(self, loop):
        if self.available():
            if loop:
                bindings.set_loop_sync_source(self.shoop_c_handle, loop.shoop_c_handle)
            else:
                bindings.set_loop_sync_source(self.shoop_c_handle, None)

    def destroy(self):
        if self.available():
            bindings.destroy_loop(self.shoop_c_handle)
            self.shoop_c_handle = None
            
    def adopt_ringbuffer_contents(self, reverse_start_cycle, cycles_length, go_to_cycle, go_to_mode):
        _reverse_start_cycle = (-1 if reverse_start_cycle == None else reverse_start_cycle)
        _cycles_length = (-1 if cycles_length == None else cycles_length)
        _go_to_cycle = (-1 if go_to_cycle == None else go_to_cycle)
        if self.available():
            bindings.adopt_ringbuffer_contents(self.shoop_c_handle, _reverse_start_cycle, _cycles_length, _go_to_cycle, go_to_mode)
        
    def __del__(self):
        if self.available():
            self.destroy()

class BackendAudioPort:
    def __init__(self, c_handle : 'POINTER(bindings.shoopdaloop_audio_port_t)',
                 input_connectability : int,
                 output_connectability : int,
                 backend : 'BackendSession'):
        self._input_connectability = input_connectability
        self._output_connectability = output_connectability
        self._c_handle = c_handle
        self._backend = backend
        self._name = '(unknown)'
        
    def available(self):
        return self._c_handle and self._backend and self._backend.active()
    
    def input_connectability(self) -> Type['PortConnectability']:
        return self._input_connectability

    def output_connectability(self) -> Type['PortConnectability']:
        return self._output_connectability
    
    def c_handle(self):
        return self._c_handle
    
    def name(self):
        return self._name
    
    def destroy(self):
        if self.available():
            bindings.destroy_audio_port(self._c_handle)
        self._c_handle = None
    
    def get_state(self):
        if self.available():
            state = bindings.get_audio_port_state(self._c_handle)
            rval = AudioPortState(deref_ptr(state))
            self._name = rval.name
            if state:
                bindings.destroy_audio_port_state_info(state)
            return rval
        else:
            return AudioPortState()
    
    def set_gain(self, gain):
        if self.available():
            bindings.set_audio_port_gain(self._c_handle, gain)
    
    def set_muted(self, muted):
        if self.available():
            bindings.set_audio_port_muted(self._c_handle, (1 if muted else 0))
    
    def set_passthrough_muted(self, muted):
        if self.available():
            bindings.set_audio_port_passthroughMuted(self._c_handle, (1 if muted else 0))
    
    def connect_internal(self, other):
            if self.available():
                bindings.connect_audio_port_internal(self._c_handle, other.c_handle())
    
    def dummy_queue_data(self, data):
        if self.available():
            data_type = c_float * len(data)
            arr = data_type()
            for i in range(len(data)):
                arr[i] = data[i]
            bindings.dummy_audio_port_queue_data(self._c_handle, len(data), arr)
        
    def dummy_dequeue_data(self, n_samples):
        if self.available():
            data_type = c_float * n_samples
            arr = data_type()
            bindings.dummy_audio_port_dequeue_data(self._c_handle, n_samples, arr)
            return [float(arr[i]) for i in range(n_samples)]
        return [0.0 for i in range(n_samples)]
    
    def dummy_request_data(self, n_samples):
        if self.available():
            bindings.dummy_audio_port_request_data(self._c_handle, n_samples)
    
    def get_connections_state(self):
        if self.available():
            state = bindings.get_audio_port_connections_state(self._c_handle)
            rval = parse_connections_state(deref_ptr(state))
            if state:
                bindings.destroy_port_connections_state(state)
            return rval
        else:
            return dict()

    def connect_external_port(self, name):
        if self.available():
            bindings.connect_audio_port_external(self._c_handle, name.encode('ascii'))
    
    def disconnect_external_port(self, name):
        if self.available():
            bindings.disconnect_audio_port_external(self._c_handle, name.encode('ascii'))
    
    def set_ringbuffer_n_samples(self, n):
        if self.available():
            bindings.set_audio_port_ringbuffer_n_samples(self._c_handle, n)

    def __del__(self):
        if self.available():
            self.destroy()

class BackendDecoupledMidiPort:
    def __init__(self, c_handle : 'POINTER(bindings.shoopdaloop_decoupled_midi_port_t)',
                 backend : 'BackendSession'):
        self._c_handle = c_handle
        self._backend = backend
        
    def available(self):
        return self._c_handle and self._backend and self._backend.active()
    
    def maybe_next_message(self):
        if self.available():
            r = bindings.maybe_next_message(self._c_handle)
            if r:
                rval = MidiEvent(r[0])
                bindings.destroy_midi_event(r)
                return rval
        return None
    
    def name(self):
        if self.available():
            return bindings.get_decoupled_midi_port_name(self._c_handle).decode('ascii')
        return '(unknown)'
    
    def destroy(self):
        if self.available():
            bindings.close_decoupled_midi_port(self._c_handle)
    
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
    
    def __del__(self):
        if self.available():
            self.destroy()
            
class BackendMidiPort:
    def __init__(self, c_handle : 'POINTER(bindings.shoopdaloop_midi_port_t)',
                 input_connectability : int,
                 output_connectability : int,
                 backend : 'BackendSession'):
        self._input_connectability = input_connectability
        self._output_connectability = output_connectability
        self._c_handle = c_handle
        self._backend = backend
        self._name = '(unknown)'
    
    def available(self):
        return self._c_handle and self._backend and self._backend.active()
    
    def input_connectability(self) -> Type['PortConnectability']:
        return self._input_connectability

    def output_connectability(self) -> Type['PortConnectability']:
        return self._output_connectability
    
    def c_handle(self):
        return self._c_handle
    
    def name(self):
        return self._name
    
    def destroy(self):
        if self.available():
            bindings.destroy_midi_port(self._c_handle)
            self._c_handle = None
    
    def get_state(self):
        if self.available():
            state = bindings.get_midi_port_state(self._c_handle)
            rval = MidiPortState(deref_ptr(state))
            self._name = rval.name
            if state:
                bindings.destroy_midi_port_state_info(state)
            return rval
        return MidiPortState()
    
    def set_muted(self, muted):
        if self.available():
            bindings.set_midi_port_muted(self._c_handle, (1 if muted else 0))
    
    def set_passthrough_muted(self, muted):
        if self.available():
            bindings.set_midi_port_passthroughMuted(self._c_handle, (1 if muted else 0))
    
    def connect_internal(self, other):
        if self.available():
            bindings.connect_midi_port_internal(self._c_handle, other.c_handle())

    def dummy_clear_queues(self):
        if self.available():
            bindings.dummy_midi_port_clear_queues(self._c_handle)

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
            bindings.dummy_midi_port_queue_data(self._c_handle, d)
            bindings.destroy_midi_sequence(d)
    
    def dummy_dequeue_data(self):
        if self.available():
            r = bindings.dummy_midi_port_dequeue_data(self._c_handle)
            if r:
                msgs = [backend_midi_message_to_dict(r[0].events[i][0]) for i in range(r[0].n_events)]
                bindings.destroy_midi_sequence(r)
                return msgs
        return []
    
    def dummy_request_data(self, n_frames):
        if self.available():
            bindings.dummy_midi_port_request_data(self._c_handle, n_frames)
    
    def get_connections_state(self):
        if self.available():
            state = bindings.get_midi_port_connections_state(self._c_handle)
            rval = parse_connections_state(deref_ptr(state))
            if state:
                bindings.destroy_port_connections_state(state)
            return rval
        return dict()

    def connect_external_port(self, name):
        if self.available():
            bindings.connect_midi_port_external(self._c_handle, name.encode('ascii'))
    
    def disconnect_external_port(self, name):
        if self.available():
            bindings.disconnect_midi_port_external(self._c_handle, name.encode('ascii'))
    
    def set_ringbuffer_n_samples(self, n):
        if self.available():
            bindings.set_midi_port_ringbuffer_n_samples(self._c_handle, n)

    def __del__(self):
        if self.available():
            self.destroy()

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

class BackendSession:
    def create():
        global all_active_backends
        _ptr = bindings.create_backend_session()
        b = BackendSession(_ptr)
        all_active_backends.add(b)
        return b
    
    def __init__(self, c_handle : 'POINTER(bindings.shoop_backend_session_t)'):
        self._c_handle = c_handle
        self._active = True

    def get_state(self):
        if self.active():
            state = bindings.get_backend_session_state(self._c_handle)
            rval = BackendSessionState(deref_ptr(state))
            if state:
                bindings.destroy_backend_state_info(state)
            return rval
        return BackendSessionState()

    def create_loop(self) -> Type['BackendLoop']:
        if self.active():
            handle = bindings.create_loop(self._c_handle)
            rval = BackendLoop(handle, self)
            return rval
        return None
    
    def create_fx_chain(self, chain_type : Type['FXChainType'], title: str) -> Type['BackendFXChain']:
        if self.active():
            handle = bindings.create_fx_chain(self._c_handle, chain_type.value, title.encode('ascii'))
            rval = BackendFXChain(handle, chain_type, self)
            return rval
        return None
    
    def get_fx_chain_audio_input_port(self, fx_chain : Type['BackendFXChain'], idx : int):
        if self.active():
            return BackendAudioPort(bindings.fx_chain_audio_input_port(fx_chain.c_handle(), idx), PortConnectability.Internal, 0, self)
        return None
    
    def get_fx_chain_audio_output_port(self, fx_chain : Type['BackendFXChain'], idx : int):
        if self.active():
            return BackendAudioPort(bindings.fx_chain_audio_output_port(fx_chain.c_handle(), idx), 0, PortConnectability.Internal, self)
        return None
    
    def get_fx_chain_midi_input_port(self, fx_chain : Type['BackendFXChain'], idx : int):
        if self.active():
            return BackendMidiPort(bindings.fx_chain_midi_input_port(fx_chain.c_handle(), idx), PortConnectability.Internal, 0, self)
        return None
    
    def get_profiling_report(self):
        if self.active():
            state = bindings.get_profiling_report(self._c_handle)
            rval = ProfilingReport(deref_ptr(state))
            if state:
                bindings.destroy_profiling_report(state)
            return rval
        return ProfilingReport()
        
    def active(self):
        return self._active

    def set_audio_driver(self, driver: Type['AudioDriver']):
        if self.active():
            result = bindings.set_audio_driver(self._c_handle, driver._c_handle)
            if result == BackendResult.Failure.value:
                raise Exception("Unable to set driver for back-end session")

    def destroy(self):
        global all_active_backends
        if self.active():
            try:
                self._active = False
                all_active_backends.remove(self)
                if self._c_handle:
                    bindings.destroy_backend_session(self._c_handle)
            except Exception:
                pass
    
    def segfault_on_process_thread(self):
        if self.active():
            bindings.do_segfault_on_process_thread(self._c_handle)
    
    def abort_on_process_thread(self):
        if self.active():
            bindings.do_abort_on_process_thread(self._c_handle)

class AudioDriver:
    def create(driver_type : Type[AudioDriverType]):
        global all_active_drivers
        _ptr = bindings.create_audio_driver(driver_type.value)
        b = AudioDriver(_ptr)
        all_active_drivers.add(b)
        return b
    
    def get_backend_obj(self):
        return self._c_handle

    def __init__(self, c_handle : 'POINTER(bindings.shoop_audio_driver_t)'):
        self._c_handle = c_handle
        self._active = False
        self._dsp_load = 0.0
        self._xruns = 0
        self._client_name = ''

    def get_state(self):
        state = bindings.get_audio_driver_state(self._c_handle)
        rval = AudioDriverState(deref_ptr(state))
        if state:
            self._active = rval.active
            self._dsp_load = rval.dsp_load
            self._xruns = rval.xruns
            self._client_name = rval.maybe_instance_name
            bindings.destroy_audio_driver_state(state)
        return rval

    def open_decoupled_midi_port(self, name_hint : str, direction : int) -> 'BackendDecoupledMidiPort':
        if self.active():
            handle = bindings.open_decoupled_midi_port(self._c_handle, name_hint.encode('ascii'), direction)
            port = BackendDecoupledMidiPort(handle, self)
            return port
        raise Exception("Trying to open a MIDI port before audio driver is started.")

    def dummy_enter_controlled_mode(self):
        if self.active():
            bindings.dummy_audio_enter_controlled_mode(self._c_handle)
        
    def dummy_enter_automatic_mode(self):
        if self.active():
            bindings.dummy_audio_enter_automatic_mode(self._c_handle)
    
    def dummy_is_controlled(self):
        if self.active():
            return bool(bindings.dummy_audio_is_in_controlled_mode(self._c_handle))
        return False
    
    def dummy_request_controlled_frames(self, n):
        if self.active():
            bindings.dummy_audio_request_controlled_frames(self._c_handle, n)
    
    def dummy_n_requested_frames(self):
        if self.active():
            return int(bindings.dummy_audio_n_requested_frames(self._c_handle))
        return 0
    
    def dummy_add_external_mock_port(self, name, direction, data_type):
        if self.active():
            bindings.dummy_driver_add_external_mock_port(self._c_handle, name.encode('ascii'), direction, data_type)
    
    def dummy_remove_external_mock_port(self, name):
        if self.active():
            bindings.dummy_driver_remove_external_mock_port(self._c_handle, name.encode('ascii'))
    
    def dummy_remove_all_external_mock_ports(self):
        if self.active():
            bindings.dummy_driver_remove_all_external_mock_ports(self._c_handle)
    
    def get_sample_rate(self):
        if self.active():
            return int(bindings.get_sample_rate(self._c_handle))
        return 1
    
    def get_buffer_size(self):
        if self.active():
            return int(bindings.get_buffer_size(self._c_handle))
        return 1

    def start_dummy(self, settings):
        bindings.start_dummy_driver(self._c_handle, settings.to_backend())
    
    def start_jack(self, settings):
        bindings.start_jack_driver(self._c_handle, settings.to_backend())
    
    def active(self):
        self.get_state()
        return self._active
    
    def wait_process(self):
        bindings.wait_process(self._c_handle)
        
    def dummy_run_requested_frames(self):
        bindings.dummy_audio_run_requested_frames(self._c_handle)
    
    def find_external_ports(self, maybe_name_regex, port_direction, data_type):
        result = bindings.find_external_ports(self._c_handle, maybe_name_regex, port_direction, data_type)
        rval = []
        for i in range(result[0].n_ports):
            rval.append(ExternalPortDescriptor(result[0].ports[i]))
        bindings.destroy_external_port_descriptors(result)
        return rval
    
    def destroy(self):
        global all_active_drivers
        if self.active():
            try:
                all_active_drivers.remove(self)
                if self._c_handle:
                    bindings.destroy_audio_driver(self._c_handle)
            except Exception:
                pass

def terminate_all_backends():
    global all_active_backends
    bs = copy.copy(all_active_backends)
    for b in bs:
        b.destroy()
    global all_active_drivers
    ds = copy.copy(all_active_drivers)
    for d in ds:
        d.destroy()

def audio_driver_type_supported(t : Type[AudioDriverType]):
    return bool(bindings.driver_type_supported(t.value))

def open_driver_audio_port(backend_session, audio_driver, name_hint : str, direction : int, min_n_ringbuffer_samples : int) -> 'BackendAudioPort':
    if backend_session.active() and audio_driver.active():
        handle = bindings.open_driver_audio_port(backend_session._c_handle, audio_driver._c_handle, name_hint.encode('ascii'), direction, min_n_ringbuffer_samples)
        port = BackendAudioPort(handle,
                                bindings.get_audio_port_input_connectability(handle),
                                bindings.get_audio_port_output_connectability(handle),
                                backend_session)
        return port
    raise Exception("Failed to open audio port: backend session or audio driver not active")

def open_driver_midi_port(backend_session, audio_driver, name_hint : str, direction : int, min_n_ringbuffer_samples : int) -> 'BackendMidiPort':
    if backend_session.active() and audio_driver.active():
        handle = bindings.open_driver_midi_port(backend_session._c_handle, audio_driver._c_handle, name_hint.encode('ascii'), direction, min_n_ringbuffer_samples)
        port = BackendMidiPort(handle,
                               bindings.get_midi_port_input_connectability(handle),
                               bindings.get_midi_port_output_connectability(handle),
                               backend_session)
        return port
    raise Exception("Failed to open MIDI port: backend session or audio driver not active")

def resample_audio(audio, target_n_frames):
    n_channels = audio.shape[1] # inner
    n_frames = audio.shape[0] # outer
    if not n_channels or not n_frames:
        return audio
    
    data_in = bindings.alloc_multichannel_audio(n_channels, n_frames)
    if not data_in:
        raise Exception('Could not allocate multichannel audio')
    for chan in range(n_channels):
        for frame in range(n_frames):
            data_in[0].data[frame*n_channels + chan] = audio[frame, chan]
    
    backend_result = bindings.resample_audio(data_in, target_n_frames)
    if not backend_result:
        raise Exception('Could not resample audio')
    result = numpy.zeros_like(audio, shape=[target_n_frames, n_channels])
    for chan in range(n_channels):
        for frame in range(target_n_frames):
            result[frame, chan] = backend_result[0].data[n_channels*frame + chan]
    bindings.destroy_multichannel_audio(backend_result)
    bindings.destroy_multichannel_audio(data_in)
    return result