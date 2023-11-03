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

try:
    from shoopdaloop.libshoopdaloop_bindings import *
except:
    # during build we may need a different path
    from libshoopdaloop_bindings import *

class PortDirection(Enum):
    Input = 0
    Output = 1

class LoopMode(Enum):
    Unknown = Unknown
    Stopped = Stopped
    Playing = Playing
    Recording = Recording
    PlayingDryThroughWet = PlayingDryThroughWet
    RecordingDryIntoWet = RecordingDryIntoWet

class ChannelMode(Enum):
    Disabled = Disabled
    Direct = Direct
    Dry = Dry
    Wet = Wet

class BackendType(Enum):
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

    def __init__(self, backend_state : "fx_chain_state_info_t"):
        self.visible = backend_state.visible
        self.active = backend_state.active
        self.ready = backend_state.ready

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

    def __init__(self, backend_state : 'loop_audio_channel_state_t'):
        self.output_peak = backend_state.output_peak
        self.volume = backend_state.volume
        self.mode = ChannelMode(backend_state.mode)
        self.length = backend_state.length
        self.start_offset = backend_state.start_offset
        self.data_dirty = bool(backend_state.data_dirty)
        self.played_back_sample = (backend_state.played_back_sample if backend_state.played_back_sample >= 0 else None)
        self.n_preplay_samples = backend_state.n_preplay_samples

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

    def __init__(self, backend_state : 'loop_midi_channel_state_t'):
        self.n_events_triggered = backend_state.n_events_triggered
        self.n_notes_active = backend_state.n_notes_active
        self.mode = ChannelMode(backend_state.mode)
        self.length = backend_state.length
        self.start_offset = backend_state.start_offset
        self.data_dirty = bool(backend_state.data_dirty)
        self.played_back_sample = (backend_state.played_back_sample if backend_state.played_back_sample >= 0 else None)
        self.n_preplay_samples = backend_state.n_preplay_samples

@dataclass
class LoopState:
    length: int
    position: int
    mode: Type[LoopMode]
    maybe_next_mode: typing.Union[Type[LoopMode], None]
    maybe_next_delay: typing.Union[int, None]

    def __init__(self, backend_loop_state : 'loop_state_t'):
        self.length = backend_loop_state.length
        self.position = backend_loop_state.position
        self.mode = backend_loop_state.mode
        self.maybe_next_mode =  (None if backend_loop_state.maybe_next_mode == LOOP_MODE_INVALID else backend_loop_state.maybe_next_mode)
        self.maybe_next_delay = (None if backend_loop_state.maybe_next_mode == LOOP_MODE_INVALID else backend_loop_state.maybe_next_mode_delay)

@dataclass
class AudioPortState:
    peak: float
    volume: float
    muted: bool
    passthrough_muted: bool
    name: str

    def __init__(self, backend_state : 'audio_port_state_info_t'):
        self.peak = backend_state.peak
        self.volume = backend_state.volume
        self.muted = bool(backend_state.muted)
        self.passthrough_muted = bool(backend_state.passthrough_muted)
        self.name = str(backend_state.name)

@dataclass
class MidiPortState:
    n_events_triggered: int
    n_notes_active : int
    muted: bool
    passthrough_muted: bool
    name: str

    def __init__(self, backend_state : 'audio_port_state_info_t'):
        self.n_events_triggered = backend_state.n_events_triggered
        self.n_notes_active = backend_state.n_notes_active
        self.muted = bool(backend_state.muted)
        self.passthrough_muted = bool(backend_state.passthrough_muted)
        self.name = str(backend_state.name)

@dataclass
class MidiEvent:
    time: int
    size: int
    data: List[int]
    
    def __init__(self, backend_event : 'midi_event_t'):
        self.time = backend_event.time
        self.size = backend_event.size
        self.data = [int(backend_event.data[i]) for i in range(backend_event.size)]

@dataclass
class BackendState:
    dsp_load_percent: float
    xruns_since_last: int
    actual_type: Type[BackendType]

    def __init__(self, backend_state : 'backend_state_info_t'):
        self.dsp_load_percent = float(backend_state.dsp_load_percent)
        self.xruns_since_last = int(backend_state.xruns_since_last)
        self.actual_type = BackendType(backend_state.actual_type)

@dataclass
class ProfilingReportItem:
    key : str
    n_samples : float
    worst : float
    most_recent : float
    average : float

    def __init__(self, backend_obj : 'profiling_report_item_t'):
        self.key = str(backend_obj.key)
        self.n_samples = float(backend_obj.n_samples)
        self.worst = float(backend_obj.worst)
        self.most_recent = float(backend_obj.most_recent)
        self.average = float(backend_obj.average)

@dataclass
class ProfilingReport:
    items : List[ProfilingReportItem]

    def __init__(self, backend_obj : 'profiling_report_t'):
        self.items = [ProfilingReportItem(backend_obj.items[i]) for i in range(backend_obj.n_items)]
        
def parse_connections_state(backend_state : 'port_connections_state_info_t'):
    rval = dict()
    for i in range(backend_state.n_ports):
        rval[str(backend_state.ports[i].name)] = bool(backend_state.ports[i].connected)
    return rval

def backend_midi_message_to_dict(backend_msg: 'midi_event_t'):
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
    def __init__(self, loop, c_handle : 'POINTER(shoopdaloop_loop_audio_channel)'):
        self.loop_shoop_c_handle = loop.c_handle()
        self.shoop_c_handle = c_handle
    
    def connect(self, port : 'BackendAudioPort'):
        if port.direction() == PortDirection.Input:
            connect_audio_input(self.shoop_c_handle, port.c_handle())
        else:
            connect_audio_output(self.shoop_c_handle, port.c_handle())
    
    def disconnect(self, port : 'BackendAudioPort'):
        if port.direction() == PortDirection.Input:
            disconnect_audio_input(self.shoop_c_handle, port.c_handle())
        else:
            disconnect_audio_output(self.shoop_c_handle, port.c_handle())
    
    def load_data(self, data):
        backend_data = alloc_audio_channel_data(len(data))
        for i in range(len(data)):
            backend_data[0].data[i] = data[i]
        load_audio_channel_data(self.shoop_c_handle, backend_data)
        destroy_audio_channel_data(backend_data)
    
    def get_data(self) -> List[float]:
        r = get_audio_channel_data(self.shoop_c_handle)
        data = [float(r[0].data[i]) for i in range(r[0].n_samples)]
        destroy_audio_channel_data(r)
        return data
    
    def get_state(self):
        state = get_audio_channel_state(self.shoop_c_handle)
        rval = LoopAudioChannelState(state[0])
        destroy_audio_channel_state_info(state)
        return rval
    
    def set_volume(self, volume):
        if self.shoop_c_handle:
            set_audio_channel_volume(self.shoop_c_handle, volume)
    
    def set_mode(self, mode : Type['ChannelMode']):
        set_audio_channel_mode(self.shoop_c_handle, mode.value)
    
    def set_start_offset(self, offset):
        if self.shoop_c_handle:
            set_audio_channel_start_offset(self.shoop_c_handle, offset)
    
    def set_n_preplay_samples(self, n):
        if self.shoop_c_handle:
            set_audio_channel_n_preplay_samples(self.shoop_c_handle, n)
    
    def destroy(self):
        if self.shoop_c_handle:
            destroy_audio_channel(self.shoop_c_handle)
            self.shoop_c_handle = None
    
    def clear_data_dirty(self):
        if self.shoop_c_handle:
            clear_audio_channel_data_dirty(self.shoop_c_handle)

    def clear(self, length=0):
        if self.shoop_c_handle:
            clear_audio_channel(self.shoop_c_handle, length)

    def __del__(self):
        self.destroy()
    
class BackendLoopMidiChannel:
    def __init__(self, loop : 'BackendLoop', c_handle : 'POINTER(shoopdaloop_loop_midi_channel_t)'):
        self.loop_shoop_c_handle = loop.c_handle()
        self.shoop_c_handle = c_handle
    
    def get_data(self):
        r = get_midi_channel_data(self.shoop_c_handle)
        msgs = [backend_midi_message_to_dict(r[0].events[i][0]) for i in range(r[0].n_events)]
        destroy_midi_sequence(r)
        return msgs
    
    def load_data(self, msgs):
        d = alloc_midi_sequence(len(msgs))
        d[0].length_samples = msgs[len(msgs)-1]['time'] + 1
        for idx, m in enumerate(msgs):
            d[0].events[idx] = midi_message_dict_to_backend(m)
        load_midi_channel_data(self.shoop_c_handle, d)
        destroy_midi_sequence(d)
    
    def connect(self, port : 'BackendMidiPort'):
        if port.direction() == PortDirection.Input:
            connect_midi_input(self.shoop_c_handle, port.c_handle())
        else:
            connect_midi_output(self.shoop_c_handle, port.c_handle())
        
    def disconnect(self, port : 'BackendMidiPort'):
        if port.direction() == PortDirection.Input:
            disconnect_midi_input(self.shoop_c_handle, port.c_handle())
        else:
            disconnect_midi_output(self.shoop_c_handle, port.c_handle())
    
    def get_state(self):
        state = get_midi_channel_state(self.shoop_c_handle)
        rval = LoopMidiChannelState(state[0])
        destroy_midi_channel_state_info(state)
        return rval
    
    def set_mode(self, mode : Type['ChannelMode']):
        set_midi_channel_mode(self.shoop_c_handle, mode.value)
    
    def set_start_offset(self, offset):
        if self.shoop_c_handle:
            set_midi_channel_start_offset(self.shoop_c_handle, offset)
    
    def set_n_preplay_samples(self, n):
        if self.shoop_c_handle:
            set_audio_channel_n_preplay_samples(self.shoop_c_handle, n)
    
    def destroy(self):
        if self.shoop_c_handle:
            destroy_midi_channel(self.shoop_c_handle)
            self.shoop_c_handle = None
    
    def clear_data_dirty(self):
        if self.shoop_c_handle:
            clear_audio_channel_data_dirty(self.shoop_c_handle)
    
    def clear(self):
        if self.shoop_c_handle:
            clear_midi_channel(self.shoop_c_handle)

    def __del__(self):
        self.destroy()

class BackendLoop:
    def __init__(self, c_handle : 'POINTER(shoopdaloop_loop_t)'):
        self.shoop_c_handle = c_handle
    
    def c_handle(self):
        return self.shoop_c_handle
    
    def add_audio_channel(self, mode : Type['ChannelMode']) -> 'BackendLoopAudioChannel':
        rval = BackendLoopAudioChannel(self, add_audio_channel(self.c_handle(), mode.value))
        return rval
    
    def add_midi_channel(self, mode : Type['ChannelMode']) -> 'BackendLoopMidiChannel':
        rval = BackendLoopMidiChannel(self, add_midi_channel(self.c_handle(), mode.value))
        return rval

    def transition(self, to_state : Type['LoopMode'],
                   cycles_delay : int, wait_for_sync : bool):
        #print('transition: {}, {}, {}'.format(to_state, cycles_delay, wait_for_sync))
        loop_transition(self.shoop_c_handle,
                                to_state.value,
                                cycles_delay,
                                wait_for_sync)
    
    # Static version for multiple loops
    def transition_multiple(loops, to_state : Type['LoopMode'],
                   cycles_delay : int, wait_for_sync : bool):
        #print('transition {} loops: {}, {}, {}'.format(len(loops), to_state, cycles_delay, wait_for_sync))
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
        state = get_loop_state(self.shoop_c_handle)
        rval = LoopState(state[0])
        destroy_loop_state_info(state)
        return rval
    
    def set_length(self, length):
        set_loop_length(self.shoop_c_handle, length)
    
    def set_position(self, position):
        set_loop_position(self.shoop_c_handle, position)
    
    def clear(self, length):
        clear_loop(self.shoop_c_handle, length)
    
    def set_sync_source(self, loop):
        if loop:
            set_loop_sync_source(self.shoop_c_handle, loop.shoop_c_handle)
        else:
            set_loop_sync_source(self.shoop_c_handle, None)

    def destroy(self):
        if self.shoop_c_handle:
            destroy_loop(self.shoop_c_handle)
            self.shoop_c_handle = None
    
    def __del__(self):
        self.destroy()

class BackendAudioPort:
    def __init__(self, c_handle : 'POINTER(shoopdaloop_audio_port_t)',
                 direction : int):
        self._direction = PortDirection(direction)
        self._c_handle = c_handle
    
    def direction(self) -> Type['PortDirection']:
        return self._direction
    
    def c_handle(self):
        return self._c_handle
    
    def name(self):
        return self.get_state().name
    
    def destroy(self):
        if self._c_handle:
            destroy_audio_port(self._c_handle)
            self._c_handle = None
    
    def get_state(self):
        state = get_audio_port_state(self._c_handle)
        rval = AudioPortState(state[0])
        destroy_audio_port_state_info(state)
        return rval
    
    def set_volume(self, volume):
        if self._c_handle:
            set_audio_port_volume(self._c_handle, volume)
    
    def set_muted(self, muted):
        if self._c_handle:
            set_audio_port_muted(self._c_handle, (1 if muted else 0))
    
    def set_passthrough_muted(self, muted):
        if self._c_handle:
            set_audio_port_passthroughMuted(self._c_handle, (1 if muted else 0))
    
    def connect_passthrough(self, other):
        add_audio_port_passthrough(self._c_handle, other.c_handle())
    
    def dummy_queue_data(self, data):
        data_type = c_float * len(data)
        arr = data_type()
        for i in range(len(data)):
            arr[i] = data[i]
        dummy_audio_port_queue_data(self._c_handle, len(data), arr)
    
    def dummy_dequeue_data(self, n_samples):
        data_type = c_float * n_samples
        arr = data_type()
        dummy_audio_port_dequeue_data(self._c_handle, n_samples, arr)
        return [float(arr[i]) for i in range(n_samples)]
    
    def dummy_request_data(self, n_samples):
        dummy_audio_port_request_data(self._c_handle, n_samples)
    
    def get_connections_state(self):
        state = get_audio_port_connections_state(self._c_handle)
        rval = parse_connections_state(state[0])
        destroy_port_connections_state(state)
        return rval

    def connect_external_port(self, name):
        connect_external_audio_port(self._c_handle, name.encode('ascii'))
    
    def disconnect_external_port(self, name):
        disconnect_external_audio_port(self._c_handle, name.encode('ascii'))

    def __del__(self):
        self.destroy()


class BackendDecoupledMidiPort:
    def __init__(self, c_handle : 'POINTER(shoopdaloop_decoupled_midi_port_t)',
                 direction : int):
        self._direction = PortDirection(direction)
        self._c_handle = c_handle
    
    def maybe_next_message(self):
        r = maybe_next_message(self._c_handle)
        if not r:
            return None
        else:
            return MidiEvent(r[0])
    
    def name(self):
        return get_decoupled_midi_port_name(self._c_handle).decode('ascii')
    
    def destroy(self):
        if self._c_handle:
            close_decoupled_midi_port(self._c_handle)
    
    def send_midi(self, msg):
        data_type = c_ubyte * len(msg)
        arr = data_type()
        for i in range(len(msg)):
            arr[i] = msg[i]
        send_decoupled_midi(self._c_handle, len(msg), arr)
    
    def __del__(self):
        self.destroy()
            
class BackendMidiPort:
    def __init__(self, c_handle : 'POINTER(shoopdaloop_midi_port_t)',
                 direction : int):
        self._direction = PortDirection(direction)
        self._c_handle = c_handle
    
    def direction(self) -> Type['PortDirection']:
        return self._direction
    
    def c_handle(self):
        return self._c_handle
    
    def name(self):
        return self.get_state().name
    
    def destroy(self):
        if self._c_handle:
            destroy_midi_port(self._c_handle)
            self._c_handle = None
    
    def get_state(self):
        state = get_midi_port_state(self._c_handle)
        rval = MidiPortState(state[0])
        destroy_midi_port_state_info(state)
        return rval
    
    def set_muted(self, muted):
        if self._c_handle:
            set_midi_port_muted(self._c_handle, (1 if muted else 0))
    
    def set_passthrough_muted(self, muted):
        if self._c_handle:
            set_midi_port_passthroughMuted(self._c_handle, (1 if muted else 0))
    
    def connect_passthrough(self, other):
        add_midi_port_passthrough(self._c_handle, other.c_handle())

    def dummy_clear_queues(self):
        dummy_midi_port_clear_queues(self._c_handle)

    def dummy_queue_msg(self, msg):
        return self.dummy_queue_msgs([msg])
    
    def dummy_queue_msgs(self, msgs):
        d = alloc_midi_sequence(len(msgs))
        d[0].length_samples = msgs[len(msgs)-1]['time'] + 1
        for idx, m in enumerate(msgs):
            d[0].events[idx] = midi_message_dict_to_backend(m)
        dummy_midi_port_queue_data(self._c_handle, d)
        destroy_midi_sequence(d)
    
    def dummy_dequeue_data(self):
        r = dummy_midi_port_dequeue_data(self._c_handle)
        msgs = [backend_midi_message_to_dict(r[0].events[i][0]) for i in range(r[0].n_events)]
        destroy_midi_sequence(r)
        return msgs
    
    def dummy_request_data(self, n_frames):
        dummy_midi_port_request_data(self._c_handle, n_frames)
    
    def get_connections_state(self):
        state = get_midi_port_connections_state(self._c_handle)
        rval = parse_connections_state(state[0])
        destroy_port_connections_state(state)
        return rval

    def connect_external_port(self, name):
        connect_external_midi_port(self._c_handle, name.encode('ascii'))
    
    def disconnect_external_port(self, name):
        disconnect_external_midi_port(self._c_handle, name.encode('ascii'))

    def __del__(self):
        self.destroy()

class BackendFXChain:
    def __init__(self, c_handle : "POINTER(shoopdaloop_fx_chain_t)", chain_type: FXChainType):
        self._type = chain_type
        self._c_handle = c_handle
    
    def chain_type(self) -> Type['FXChainType']:
        return self._type
    
    def c_handle(self):
        return self._c_handle
    
    def set_visible(self, visible):
        if self._c_handle:
            fx_chain_set_ui_visible(self._c_handle, int(visible))
    
    def set_active(self, active):
        if self._c_handle:
            set_fx_chain_active(self._c_handle, int(active))
    
    def get_state(self):
        state = get_fx_chain_state(self._c_handle)
        rval = FXChainState(state[0])
        destroy_fx_chain_state(state)
        return rval
    
    def get_state_str(self):
        state = get_fx_chain_internal_state(self._c_handle)
        rval = state.decode('ascii')
        # TODO destroy_string(state)
        return rval
    
    def restore_state(self, state_str):
        restore_fx_chain_internal_state(self._c_handle, c_char_p(bytes(state_str, 'ascii')))

class Backend:
    def __init__(self, c_handle : 'POINTER(shoopdaloop_backend_instance_t)'):
        self._c_handle = c_handle

    def maybe_shoop_jack_client_handle(self):
        return maybe_jack_client_handle(self._c_handle)

    def get_client_name(self):
        return str(get_jack_client_name(self._c_handle).decode('ascii'))

    def get_sample_rate(self):
        return int(get_sample_rate(self._c_handle))
    
    def get_state(self):
        state = get_backend_state(self._c_handle)
        rval = BackendState(state[0])
        destroy_backend_state_info(state)
        return rval

    def create_loop(self) -> Type['BackendLoop']:
        handle = create_loop(self._c_handle)
        rval = BackendLoop(handle)
        return rval
    
    def create_fx_chain(self, chain_type : Type['FXChainType'], title: str) -> Type['BackendFXChain']:
        handle = create_fx_chain(self._c_handle, chain_type.value, title.encode('ascii'))
        rval = BackendFXChain(handle, chain_type)
        return rval

    def open_audio_port(self, name_hint : str, direction : int) -> 'BackendAudioPort':
        _dir = (Input if direction == PortDirection.Input.value else Output)
        handle = open_audio_port(self._c_handle, name_hint.encode('ascii'), _dir)
        port = BackendAudioPort(handle, direction)
        return port

    def open_jack_midi_port(self, name_hint : str, direction : int) -> 'BackendMidiPort':
        _dir = (Input if direction == PortDirection.Input.value else Output)
        handle = open_midi_port(self._c_handle, name_hint.encode('ascii'), _dir)
        port = BackendMidiPort(handle, direction)
        return port

    def open_decoupled_midi_port(self, name_hint : str, direction : int) -> 'BackendDecoupledMidiPort':
        _dir = (Input if direction == PortDirection.Input.value else Output)
        handle = open_decoupled_midi_port(self._c_handle, name_hint.encode('ascii'), _dir)
        port = BackendDecoupledMidiPort(handle, direction)
        return port
    
    def get_fx_chain_audio_input_port(self, fx_chain : Type['BackendFXChain'], idx : int):
        return BackendAudioPort(fx_chain_audio_input_port(fx_chain.c_handle(), idx), PortDirection.Output)
    
    def get_fx_chain_audio_output_port(self, fx_chain : Type['BackendFXChain'], idx : int):
        return BackendAudioPort(fx_chain_audio_output_port(fx_chain.c_handle(), idx), PortDirection.Input)
    
    def get_fx_chain_midi_input_port(self, fx_chain : Type['BackendFXChain'], idx : int):
        return BackendMidiPort(fx_chain_midi_input_port(fx_chain.c_handle(), idx), PortDirection.Output)
        
    def get_sample_rate(self):
        return int(get_sample_rate(self._c_handle))
    
    def get_buffer_size(self):
        return int(get_buffer_size(self._c_handle))
    
    def get_profiling_report(self):
        state = get_profiling_report(self._c_handle)
        rval = ProfilingReport(state[0])
        destroy_profiling_report(state)
        return rval

    def dummy_enter_controlled_mode(self):
        dummy_audio_enter_controlled_mode(self._c_handle)
        
    def dummy_enter_automatic_mode(self):
        dummy_audio_enter_automatic_mode(self._c_handle)
        
    def dummy_is_controlled(self):
        return bool(dummy_audio_is_in_controlled_mode(self._c_handle))
    
    def dummy_request_controlled_frames(self, n):
        dummy_audio_request_controlled_frames(self._c_handle, n)
    
    def dummy_n_requested_frames(self):
        return int(dummy_audio_n_requested_frames(self._c_handle))

    def dummy_wait_process(self):
        dummy_audio_wait_process(self._c_handle)

    def terminate(self):
        terminate(self._c_handle)

def init_backend(backend_type : Type[BackendType], client_name_hint : str, argstring : str):
    _ptr = initialize(backend_type.value, client_name_hint.encode('ascii'), argstring.encode('ascii'))
    b = Backend(_ptr)
    return b