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

script_dir = os.path.dirname(__file__)
all_active_backends = set()
all_active_drivers = set()

try:
    # On Windows, shoopdaloop.dll depends on shared libraries in the same folder.
    # this folder needs to be added to the PATH before loading
    os.environ['PATH'] = os.environ['PATH'] + os.pathsep + os.path.join(script_dir, '..')
    from shoopdaloop.libshoopdaloop_bindings import *
    from shoopdaloop.libshoopdaloop_bindings import Input as _Input, Output as _Output, Success as _Success, Failure as _Failure
except:
    # during build we may need a different path
    os.environ['PATH'] = os.environ['PATH'] + os.pathsep + script_dir
    from libshoopdaloop_bindings import *
    from libshoopdaloop_bindings import Input as _Input, Output as _Output, Success as _Success, Failure as _Failure

initialize_logging()

class PortDirection(Enum):
    Input = _Input
    Output = _Output

class BackendResult(Enum):
    Success = _Success
    Failure = _Failure
class LoopMode(Enum):
    Unknown = LoopMode_Unknown
    Stopped = LoopMode_Stopped
    Playing = LoopMode_Playing
    Recording = LoopMode_Recording
    PlayingDryThroughWet = LoopMode_PlayingDryThroughWet
    RecordingDryIntoWet = LoopMode_RecordingDryIntoWet

class ChannelMode(Enum):
    Disabled = ChannelMode_Disabled
    Direct = ChannelMode_Direct
    Dry = ChannelMode_Dry
    Wet = ChannelMode_Wet

class AudioDriverType(Enum):
    Jack = Jack
    JackTest = JackTest
    Dummy = Dummy

class FXChainType(Enum):
    Carla_Rack = Carla_Rack
    Carla_Patchbay = Carla_Patchbay
    Carla_Patchbay_16x = Carla_Patchbay_16x
    Test2x2x1 = Test2x2x1

@dataclass
class FXChainState:
    visible : bool
    ready : bool
    active : bool

    def __init__(self, backend_state : "shoop_fx_chain_state_info_t" = None):
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
    volume : float
    length : int
    start_offset : int
    data_dirty : bool
    played_back_sample : Any
    n_preplay_samples : int

    def __init__(self, backend_state : 'loop_audio_channel_state_t' = None):
        if backend_state:
            self.output_peak = backend_state.output_peak
            self.volume = backend_state.volume
            self.mode = ChannelMode(backend_state.mode)
            self.length = backend_state.length
            self.start_offset = backend_state.start_offset
            self.data_dirty = bool(backend_state.data_dirty)
            self.played_back_sample = (backend_state.played_back_sample if backend_state.played_back_sample >= 0 else None)
            self.n_preplay_samples = backend_state.n_preplay_samples
        else:
            self.output_peak = 0.0
            self.volume = 0.0
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

    def __init__(self, backend_state : 'loop_midi_channel_state_t' = None):
        if backend_state:
            self.n_events_triggered = backend_state.n_events_triggered
            self.n_notes_active = backend_state.n_notes_active
            self.mode = ChannelMode(backend_state.mode)
            self.length = backend_state.length
            self.start_offset = backend_state.start_offset
            self.data_dirty = bool(backend_state.data_dirty)
            self.played_back_sample = (backend_state.played_back_sample if backend_state.played_back_sample >= 0 else None)
            self.n_preplay_samples = backend_state.n_preplay_samples
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

    def __init__(self, backend_loop_state : 'loop_state_t' = None):
        if backend_loop_state:
            self.length = backend_loop_state.length
            self.position = backend_loop_state.position
            self.mode = backend_loop_state.mode
            self.maybe_next_mode =  (None if backend_loop_state.maybe_next_mode == LOOP_MODE_INVALID else backend_loop_state.maybe_next_mode)
            self.maybe_next_delay = (None if backend_loop_state.maybe_next_mode == LOOP_MODE_INVALID else backend_loop_state.maybe_next_mode_delay)
        else:
            self.length = 0
            self.position = 0
            self.mode = LoopMode.Unknown
            self.maybe_next_mode = None
            self.maybe_next_delay = None
@dataclass
class AudioPortState:
    peak: float
    volume: float
    muted: bool
    passthrough_muted: bool
    name: str

    def __init__(self, backend_state : 'shoop_audio_port_state_info_t' = None):
        if backend_state:
            self.peak = backend_state.peak
            self.volume = backend_state.volume
            self.muted = bool(backend_state.muted)
            self.passthrough_muted = bool(backend_state.passthrough_muted)
            self.name = str(backend_state.name)
        else:
            self.peak = 0.0
            self.volume = 0.0
            self.muted = False
            self.passthrough_muted = False
            self.name = '(unknown)'

@dataclass
class MidiPortState:
    n_events_triggered: int
    n_notes_active : int
    muted: bool
    passthrough_muted: bool
    name: str

    def __init__(self, backend_state : 'shoop_audio_port_state_info_t' = None):
        if backend_state:
            self.n_events_triggered = backend_state.n_events_triggered
            self.n_notes_active = backend_state.n_notes_active
            self.muted = bool(backend_state.muted)
            self.passthrough_muted = bool(backend_state.passthrough_muted)
            self.name = str(backend_state.name)
        else:
            self.n_events_triggered = 0
            self.n_notes_active = 0
            self.muted = False
            self.passthrough_muted = False
            self.name = '(unknown)'

@dataclass
class MidiEvent:
    time: int
    size: int
    data: List[int]
    
    def __init__(self, backend_event : 'shoop_midi_event_t'):
        self.time = backend_event.time
        self.size = backend_event.size
        self.data = [int(backend_event.data[i]) for i in range(backend_event.size)]

@dataclass
class BackendSessionState:
    audio_driver_handle : Any

    def __init__(self, backend_state : 'shoop_backend_session_state_info_t' = None):
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

    def __init__(self, backend_obj : 'shoop_profiling_report_item_t'):
        self.key = str(backend_obj.key)
        self.n_samples = float(backend_obj.n_samples)
        self.worst = float(backend_obj.worst)
        self.most_recent = float(backend_obj.most_recent)
        self.average = float(backend_obj.average)

@dataclass
class ProfilingReport:
    items : List[ProfilingReportItem]

    def __init__(self, backend_obj : 'shoop_profiling_report_t'):
        self.items = [ProfilingReportItem(backend_obj.items[i]) for i in range(backend_obj.n_items)]
    
    def __init__(self):
        self.items = []

@dataclass
class AudioDriverState:
    dsp_load : float
    xruns : int
    maybe_driver_handle : Any
    maybe_instance_name : str
    sample_rate : int
    buffer_size : int
    
    def __init__(self, backend_obj : 'shoop_audio_driver_state_t'):
        self.dsp_load = float(backend_obj.dsp_load)
        self.xruns = int(backend_obj.xruns_since_last)
        self.maybe_driver_handle = backend_obj.maybe_driver_handle
        self.maybe_instance_name = str(backend_obj.maybe_instance_name)
        self.sample_rate = int(backend_obj.sample_rate)
        self.buffer_size = int(backend_obj.buffer_size)
        
def parse_connections_state(backend_state : 'port_connections_state_info_t'):
    rval = dict()
    for i in range(backend_state.n_ports):
        rval[str(backend_state.ports[i].name)] = bool(backend_state.ports[i].connected)
    return rval

def backend_midi_message_to_dict(backend_msg: 'shoop_midi_event_t'):
    r = dict()
    r['time'] = backend_msg.time
    r['data'] = [int(backend_msg.data[i]) for i in range(backend_msg.size)]
    return r

def midi_message_dict_to_backend(msg):
    m = alloc_midi_event(len(msg['data']))
    m[0].time = msg['time']
    m[0].size = len(msg['data'])
    for i in range(len(msg['data'])):
        m[0].data[i] = msg['data'][i]
    return m

class BackendLoopAudioChannel:
    def __init__(self, loop, c_handle : 'POINTER(shoopdaloop_loop_audio_channel)', backend: 'Backend'):
        self.loop_shoop_c_handle = loop.c_handle()
        self.shoop_c_handle = c_handle
        self._backend = backend
    
    def available(self):
        return self.shoop_c_handle and self.loop_shoop_c_handle and self._backend and self._backend.active()
    
    def connect(self, port : 'BackendAudioPort'):
        if self.available():
            if port.direction() == PortDirection.Input:
                connect_audio_input(self.shoop_c_handle, port.c_handle())
            else:
                connect_audio_output(self.shoop_c_handle, port.c_handle())
    
    def disconnect(self, port : 'BackendAudioPort'):
        if self.available():
            if port.direction() == PortDirection.Input:
                disconnect_audio_input(self.shoop_c_handle, port.c_handle())
            else:
                disconnect_audio_output(self.shoop_c_handle, port.c_handle())
    
    def load_data(self, data):
        if self.available():
            backend_data = alloc_audio_channel_data(len(data))
            for i in range(len(data)):
                backend_data[0].data[i] = data[i]
            load_audio_channel_data(self.shoop_c_handle, backend_data)
            destroy_audio_channel_data(backend_data)
    
    def get_data(self) -> List[float]:
        if self.available():
            r = get_audio_channel_data(self.shoop_c_handle)
            data = [float(r[0].data[i]) for i in range(r[0].n_samples)]
            destroy_audio_channel_data(r)
            return data
        return []
    
    def get_state(self):
        if self.available():
            state = get_audio_channel_state(self.shoop_c_handle)
            rval = LoopAudioChannelState(state[0])
            destroy_audio_channel_state_info(state)
            return rval
        return LoopAudioChannelState()
    
    def set_volume(self, volume):
        if self.available():
            set_audio_channel_volume(self.shoop_c_handle, volume)
    
    def set_mode(self, mode : Type['ChannelMode']):
        if self.available():
            set_audio_channel_mode(self.shoop_c_handle, mode.value)
    
    def set_start_offset(self, offset):
        if self.available():
            set_audio_channel_start_offset(self.shoop_c_handle, offset)
    
    def set_n_preplay_samples(self, n):
        if self.available():
            set_audio_channel_n_preplay_samples(self.shoop_c_handle, n)
    
    def destroy(self):
        if self.available():
            destroy_audio_channel(self.shoop_c_handle)
            self.shoop_c_handle = None
    
    def clear_data_dirty(self):
        if self.available():
            clear_audio_channel_data_dirty(self.shoop_c_handle)

    def clear(self, length=0):
        if self.available():
            clear_audio_channel(self.shoop_c_handle, length)

    def __del__(self):
        if self.available():
            self.destroy()
    
class BackendLoopMidiChannel:
    def __init__(self, loop : 'BackendLoop', c_handle : 'POINTER(shoopdaloop_loop_midi_channel_t)',
                 backend: 'BackendSession'):
        self.loop_shoop_c_handle = loop.c_handle()
        self.shoop_c_handle = c_handle
        self._backend = backend
    
    def available(self):
        return self.shoop_c_handle and self.loop_shoop_c_handle and self._backend and self._backend.active()
    
    def get_data(self):
        if self.available():
            r = get_midi_channel_data(self.shoop_c_handle)
            msgs = [backend_midi_message_to_dict(r[0].events[i][0]) for i in range(r[0].n_events)]
            destroy_midi_sequence(r)
            return msgs
        return []
    
    def load_data(self, msgs):
        if self.available():
            d = alloc_midi_sequence(len(msgs))
            d[0].length_samples = msgs[len(msgs)-1]['time'] + 1
            for idx, m in enumerate(msgs):
                d[0].events[idx] = midi_message_dict_to_backend(m)
            load_midi_channel_data(self.shoop_c_handle, d)
            destroy_midi_sequence(d)
    
    def connect(self, port : 'BackendMidiPort'):
        if self.available():
            if port.direction() == PortDirection.Input:
                connect_midi_input(self.shoop_c_handle, port.c_handle())
            else:
                connect_midi_output(self.shoop_c_handle, port.c_handle())
        
    def disconnect(self, port : 'BackendMidiPort'):
        if self.available():
            if port.direction() == PortDirection.Input:
                disconnect_midi_input(self.shoop_c_handle, port.c_handle())
            else:
                disconnect_midi_output(self.shoop_c_handle, port.c_handle())
    
    def get_state(self):
        if self.available():
            state = get_midi_channel_state(self.shoop_c_handle)
            rval = LoopMidiChannelState(state[0])
            destroy_midi_channel_state_info(state)
            return rval
        return LoopMidiChannelState()
    
    def set_mode(self, mode : Type['ChannelMode']):
        if self.available():
            set_midi_channel_mode(self.shoop_c_handle, mode.value)
    
    def set_start_offset(self, offset):
        if self.available():
            set_midi_channel_start_offset(self.shoop_c_handle, offset)
    
    def set_n_preplay_samples(self, n):
        if self.available():
            set_audio_channel_n_preplay_samples(self.shoop_c_handle, n)
    
    def destroy(self):
        if self.available():
            destroy_midi_channel(self.shoop_c_handle)
            self.shoop_c_handle = None
    
    def clear_data_dirty(self):
        if self.available():
            clear_audio_channel_data_dirty(self.shoop_c_handle)
    
    def clear(self):
        if self.available():
            clear_midi_channel(self.shoop_c_handle)

    def __del__(self):
        if self.available():
            self.destroy()

class BackendLoop:
    def __init__(self, c_handle : 'POINTER(shoopdaloop_loop_t)', backend: 'BackendSession'):
        self.shoop_c_handle = c_handle
        self._backend = backend
    
    def available(self):
        return self.shoop_c_handle and self._backend and self._backend.active()
    
    def c_handle(self):
        return self.shoop_c_handle
    
    def add_audio_channel(self, mode : Type['ChannelMode']) -> 'BackendLoopAudioChannel':
        if self.available():
            rval = BackendLoopAudioChannel(self, add_audio_channel(self.c_handle(), mode.value), self._backend)
            return rval
        return None
    
    def add_midi_channel(self, mode : Type['ChannelMode']) -> 'BackendLoopMidiChannel':
        if self.available():
            rval = BackendLoopMidiChannel(self, add_midi_channel(self.c_handle(), mode.value), self._backend)
            return rval
        return None

    def transition(self, to_state : Type['LoopMode'],
                   cycles_delay : int, wait_for_sync : bool):
        if self.available():
            loop_transition(self.shoop_c_handle,
                                    to_state.value,
                                    cycles_delay,
                                    wait_for_sync)
    
    # Static version for multiple loops
    def transition_multiple(loops, to_state : Type['LoopMode'],
                   cycles_delay : int, wait_for_sync : bool):
        backend = loops[0]._backend
        if backend and backend.active():
            HandleType = POINTER(shoopdaloop_loop_t)
            handles = (HandleType * len(loops))()
            for idx,l in enumerate(loops):
                handles[idx] = l.c_handle()
            loops_transition(len(loops),
                                    handles,
                                    to_state.value,
                                    cycles_delay,
                                    wait_for_sync)
            del handles
    
    def get_state(self):
        if self.available():
            state = get_loop_state(self.shoop_c_handle)
            rval = LoopState(state[0])
            destroy_loop_state_info(state)
            return rval
        return LoopState()
    
    def set_length(self, length):
        if self.available():
            set_loop_length(self.shoop_c_handle, length)
    
    def set_position(self, position):
        if self.available():
            set_loop_position(self.shoop_c_handle, position)
    
    def clear(self, length):
        if self.available():
            clear_loop(self.shoop_c_handle, length)
    
    def set_sync_source(self, loop):
        if self.available():
            if loop:
                set_loop_sync_source(self.shoop_c_handle, loop.shoop_c_handle)
            else:
                set_loop_sync_source(self.shoop_c_handle, None)

    def destroy(self):
        if self.available():
            destroy_loop(self.shoop_c_handle)
            self.shoop_c_handle = None
    
    def __del__(self):
        if self.available():
            self.destroy()

class BackendAudioPort:
    def __init__(self, c_handle : 'POINTER(shoopdaloop_audio_port_t)',
                 direction : int,
                 backend : 'BackendSession'):
        self._direction = PortDirection(direction)
        self._c_handle = c_handle
        self._backend = backend
        self._name = '(unknown)'
        
    def available(self):
        return self._c_handle and self._backend and self._backend.active()
    
    def direction(self) -> Type['PortDirection']:
        return self._direction
    
    def c_handle(self):
        return self._c_handle
    
    def name(self):
        return self._name
    
    def destroy(self):
        if self.available():
            destroy_audio_port(self._c_handle)
        self._c_handle = None
    
    def get_state(self):
        if self.available():
            state = get_audio_port_state(self._c_handle)
            rval = AudioPortState(state[0])
            self._name = rval.name
            destroy_audio_port_state_info(state)
            return rval
        else:
            return AudioPortState()
    
    def set_volume(self, volume):
        if self.available():
            set_audio_port_volume(self._c_handle, volume)
    
    def set_muted(self, muted):
        if self.available():
            set_audio_port_muted(self._c_handle, (1 if muted else 0))
    
    def set_passthrough_muted(self, muted):
        if self.available():
            set_audio_port_passthroughMuted(self._c_handle, (1 if muted else 0))
    
    def connect_passthrough(self, other):
        if self.available():
            add_audio_port_passthrough(self._c_handle, other.c_handle())
    
    def dummy_queue_data(self, data):
        if self.available():
            data_type = c_float * len(data)
            arr = data_type()
            for i in range(len(data)):
                arr[i] = data[i]
            dummy_audio_port_queue_data(self._c_handle, len(data), arr)
    
    def dummy_dequeue_data(self, n_samples):
        if self.available():
            data_type = c_float * n_samples
            arr = data_type()
            dummy_audio_port_dequeue_data(self._c_handle, n_samples, arr)
            return [float(arr[i]) for i in range(n_samples)]
        return [0.0 for i in range(n_samples)]
    
    def dummy_request_data(self, n_samples):
        if self.available():
            dummy_audio_port_request_data(self._c_handle, n_samples)
    
    def get_connections_state(self):
        if self.available():
            state = get_audio_port_connections_state(self._c_handle)
            rval = parse_connections_state(state[0])
            destroy_port_connections_state(state)
            return rval
        else:
            return dict()

    def connect_external_port(self, name):
        if self.available():
            connect_external_audio_port(self._c_handle, name.encode('ascii'))
    
    def disconnect_external_port(self, name):
        if self.available():
            disconnect_external_audio_port(self._c_handle, name.encode('ascii'))

    def __del__(self):
        if self.available():
            self.destroy()


class BackendDecoupledMidiPort:
    def __init__(self, c_handle : 'POINTER(shoopdaloop_decoupled_midi_port_t)',
                 direction : int,
                 backend : 'BackendSession'):
        self._direction = PortDirection(direction)
        self._c_handle = c_handle
        self._backend = backend
        
    def available(self):
        return self._c_handle and self._backend and self._backend.active()
    
    def maybe_next_message(self):
        if self.available():
            r = maybe_next_message(self._c_handle)
            return (MidiEvent(r[0]) if r else None)
        return None
    
    def name(self):
        if self.available():
            return get_decoupled_midi_port_name(self._c_handle).decode('ascii')
        return '(unknown)'
    
    def destroy(self):
        if self.available():
            close_decoupled_midi_port(self._c_handle)
    
    def send_midi(self, msg):
        if self.available():
            data_type = c_ubyte * len(msg)
            arr = data_type()
            for i in range(len(msg)):
                arr[i] = msg[i]
            send_decoupled_midi(self._c_handle, len(msg), arr)
    
    def __del__(self):
        if self.available():
            self.destroy()
            
class BackendMidiPort:
    def __init__(self, c_handle : 'POINTER(shoopdaloop_midi_port_t)',
                 direction : int,
                 backend : 'BackendSession'):
        self._direction = PortDirection(direction)
        self._c_handle = c_handle
        self._backend = backend
        self._name = '(unknown)'
    
    def available(self):
        return self._c_handle and self._backend and self._backend.active()
    
    def direction(self) -> Type['PortDirection']:
        return self._direction
    
    def c_handle(self):
        return self._c_handle
    
    def name(self):
        return self._name
    
    def destroy(self):
        if self.available():
            destroy_midi_port(self._c_handle)
            self._c_handle = None
    
    def get_state(self):
        if self.available():
            state = get_midi_port_state(self._c_handle)
            rval = MidiPortState(state[0])
            self._name = rval.name
            destroy_midi_port_state_info(state)
            return rval
        return MidiPortState()
    
    def set_muted(self, muted):
        if self.available():
            set_midi_port_muted(self._c_handle, (1 if muted else 0))
    
    def set_passthrough_muted(self, muted):
        if self.available():
            set_midi_port_passthroughMuted(self._c_handle, (1 if muted else 0))
    
    def connect_passthrough(self, other):
        if self.available():
            add_midi_port_passthrough(self._c_handle, other.c_handle())

    def dummy_clear_queues(self):
        if self.available():
            dummy_midi_port_clear_queues(self._c_handle)

    def dummy_queue_msg(self, msg):
        if self.available():
            return self.dummy_queue_msgs([msg])
    
    def dummy_queue_msgs(self, msgs):
        if self.available():
            d = alloc_midi_sequence(len(msgs))
            d[0].length_samples = msgs[len(msgs)-1]['time'] + 1
            for idx, m in enumerate(msgs):
                d[0].events[idx] = midi_message_dict_to_backend(m)
            dummy_midi_port_queue_data(self._c_handle, d)
            destroy_midi_sequence(d)
    
    def dummy_dequeue_data(self):
        if self.available():
            r = dummy_midi_port_dequeue_data(self._c_handle)
            msgs = [backend_midi_message_to_dict(r[0].events[i][0]) for i in range(r[0].n_events)]
            destroy_midi_sequence(r)
            return msgs
        return []
    
    def dummy_request_data(self, n_frames):
        if self.available():
            dummy_midi_port_request_data(self._c_handle, n_frames)
    
    def get_connections_state(self):
        if self.available():
            state = get_midi_port_connections_state(self._c_handle)
            rval = parse_connections_state(state[0])
            destroy_port_connections_state(state)
            return rval
        return dict()

    def connect_external_port(self, name):
        if self.available():
            connect_external_midi_port(self._c_handle, name.encode('ascii'))
    
    def disconnect_external_port(self, name):
        if self.available():
            disconnect_external_midi_port(self._c_handle, name.encode('ascii'))

    def __del__(self):
        if self.available():
            self.destroy()

class BackendFXChain:
    def __init__(self, c_handle : "POINTER(shoopdaloop_fx_chain_t)", chain_type: FXChainType,
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
            fx_chain_set_ui_visible(self._c_handle, int(visible))
    
    def set_active(self, active):
        if self.available():
            set_fx_chain_active(self._c_handle, int(active))
    
    def get_state(self):
        if self.available():
            state = get_fx_chain_state(self._c_handle)
            rval = FXChainState(state[0])
            destroy_fx_chain_state(state)
            return rval
        return FXChainState()
    
    def get_state_str(self):
        if self.available():
            state = get_fx_chain_internal_state(self._c_handle)
            rval = state.decode('ascii')
            # TODO destroy_string(state)
            return rval
        return ''
    
    def restore_state(self, state_str):
        if self.available():
            restore_fx_chain_internal_state(self._c_handle, c_char_p(bytes(state_str, 'ascii')))

class BackendSession:
    def create():
        global all_active_backends
        _ptr = create_backend_session()
        b = BackendSession(_ptr)
        all_active_backends.add(b)
        return b
    
    def __init__(self, c_handle : 'POINTER(shoop_backend_session_t)'):
        self._c_handle = c_handle
        self._active = True

    def get_state(self):
        if self.active():
            state = get_backend_session_state(self._c_handle)
            rval = BackendSessionState(state[0])
            destroy_backend_state_info(state)
            return rval
        return BackendSessionState()

    def create_loop(self) -> Type['BackendLoop']:
        if self.active():
            handle = create_loop(self._c_handle)
            rval = BackendLoop(handle, self)
            return rval
        return None
    
    def create_fx_chain(self, chain_type : Type['FXChainType'], title: str) -> Type['BackendFXChain']:
        if self.active():
            handle = create_fx_chain(self._c_handle, chain_type.value, title.encode('ascii'))
            rval = BackendFXChain(handle, chain_type, self)
            return rval
        return None

    def open_decoupled_midi_port(self, name_hint : str, direction : int) -> 'BackendDecoupledMidiPort':
        if self.active():
            _dir = (Input if direction == PortDirection.Input.value else Output)
            handle = open_decoupled_midi_port(self._c_handle, name_hint.encode('ascii'), _dir)
            port = BackendDecoupledMidiPort(handle, direction, self)
            return port
        return None
    
    def get_fx_chain_audio_input_port(self, fx_chain : Type['BackendFXChain'], idx : int):
        if self.active():
            return BackendAudioPort(fx_chain_audio_input_port(fx_chain.c_handle(), idx), PortDirection.Output, self)
        return None
    
    def get_fx_chain_audio_output_port(self, fx_chain : Type['BackendFXChain'], idx : int):
        if self.active():
            return BackendAudioPort(fx_chain_audio_output_port(fx_chain.c_handle(), idx), PortDirection.Input, self)
        return None
    
    def get_fx_chain_midi_input_port(self, fx_chain : Type['BackendFXChain'], idx : int):
        if self.active():
            return BackendMidiPort(fx_chain_midi_input_port(fx_chain.c_handle(), idx), PortDirection.Output, self)
        return None
    
    def get_profiling_report(self):
        if self.active():
            state = get_profiling_report(self._c_handle)
            rval = ProfilingReport(state[0])
            destroy_profiling_report(state)
            return rval
        return ProfilingReport()
        
    def active(self):
        return self._active

    def set_audio_driver(self, driver: Type['AudioDriver']):
        if self.active():
            result = set_audio_driver(self._c_handle, driver._c_handle)
            if result == BackendResult.Failure.value:
                raise Exception("Unable to set driver for back-end session")

    def destroy(self):
        global all_active_backends
        if self.active():
            try:
                self._active = False
                all_active_backends.remove(self)
                if self._c_handle:
                    destroy_backend_session(self._c_handle)
            except Exception:
                pass

class AudioDriver:
    def create(driver_type : Type[AudioDriverType]):
        global all_active_drivers
        _ptr = create_audio_driver(driver_type.value)
        b = AudioDriver(_ptr)
        all_active_drivers.add(b)
        return b

    def __init__(self, c_handle : 'POINTER(shoop_audio_driver_t)'):
        self._c_handle = c_handle

    def get_state(self):
        if self.active():
            state = get_audio_driver_state(self._c_handle)
            rval = AudioDriverState(state[0])
            destroy_audio_driver_state(state)
            return rval
        return AudioDriverState()

    def dummy_enter_controlled_mode(self):
        if self.active():
            dummy_audio_enter_controlled_mode(self._c_handle)
        
    def dummy_enter_automatic_mode(self):
        if self.active():
            dummy_audio_enter_automatic_mode(self._c_handle)
        
    def dummy_is_controlled(self):
        if self.active():
            return bool(dummy_audio_is_in_controlled_mode(self._c_handle))
        return False
    
    def dummy_request_controlled_frames(self, n):
        if self.active():
            dummy_audio_request_controlled_frames(self._c_handle, n)
    
    def dummy_n_requested_frames(self):
        if self.active():
            return int(dummy_audio_n_requested_frames(self._c_handle))
        return 0

    def dummy_wait_process(self):
        if self.active():
            dummy_audio_wait_process(self._c_handle)
    
    def get_sample_rate(self):
        if self.active():
            return int(get_sample_rate(self._c_handle))
        return 1
    
    def get_buffer_size(self):
        if self.active():
            return int(get_buffer_size(self._c_handle))
        return 1
    
    def destroy(self):
        global all_active_drivers
        if self.active():
            try:
                all_active_drivers.remove(self)
                if self._c_handle:
                    destroy_audio_driver(self._c_handle)
            except Exception:
                pass

def terminate_all_backends():
    global all_active_backends
    bs = copy.copy(all_active_backends)
    for b in bs:
        b.terminate()

def audio_driver_type_supported(t : Type[AudioDriverType]):
    return bool(driver_type_supported(t.value))

def open_audio_port(backend_session, audio_driver, name_hint : str, direction : int) -> 'BackendAudioPort':
    if backend_session.active() and audio_driver.active() and backend_session.get_audio_driver() == audio_driver:
        _dir = (Input if direction == PortDirection.Input.value else Output)
        handle = open_audio_port(backend_session._c_handle, audio_driver._c_handle, name_hint.encode('ascii'), _dir)
        port = BackendAudioPort(handle, direction, backend_session)
        return port
    return None

def open_jack_midi_port(backend_session, audio_driver, name_hint : str, direction : int) -> 'BackendMidiPort':
    if backend_session.active() and audio_driver.active() and backend_session.get_audio_driver() == audio_driver:
        _dir = (Input if direction == PortDirection.Input.value else Output)
        handle = open_midi_port(backend_session._c_handle, audio_driver._c_handle, name_hint.encode('ascii'), _dir)
        port = BackendMidiPort(handle, direction, backend_session)
        return port
    return None