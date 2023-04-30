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

import jacklib
import mido
from PySide6.QtCore import QObject

from .backend_interface import *

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
    Dummy = Dummy

class FXChainType(Enum):
    Carla_Rack = Carla_Rack
    Carla_Patchbay = Carla_Patchbay
    Carla_Patchbay_16x = Carla_Patchbay_16x

@dataclass
class FXChainState:
    visible : bool
    running : bool

    def __init__(self, backend_state : "fx_chain_state_info_t"):
        self.visible = backend_state.visible
        self.running = backend_state.running

@dataclass
class LoopAudioChannelState:
    mode : Type[ChannelMode]
    output_peak : float
    volume : float
    length : int
    start_offset : int

    def __init__(self, backend_state : 'loop_audio_channel_state_t'):
        self.output_peak = backend_state.output_peak
        self.volume = backend_state.volume
        self.mode = ChannelMode(backend_state.mode)
        self.length = backend_state.length
        self.start_offset = backend_state.start_offset

@dataclass
class LoopMidiChannelState:
    mode: Type[ChannelMode]
    n_events_triggered : int
    n_notes_active : int
    length: int
    start_offset: int

    def __init__(self, backend_state : 'loop_midi_channel_state_t'):
        self.n_events_triggered = backend_state.n_events_triggered
        self.n_notes_active = backend_state.n_notes_active
        self.mode = ChannelMode(backend_state.mode)
        self.length = backend_state.length
        self.start_offset = backend_state.start_offset

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
    passthrough_volume: float
    muted: bool
    passthrough_muted: bool
    name: str

    def __init__(self, backend_state : 'audio_port_state_info_t'):
        self.peak = backend_state.peak
        self.volume = backend_state.volume
        self.passthrough_volume = backend_state.passthrough_volume
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
    
    def get_data(self) -> list[float]:
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
    
    def destroy(self):
        if self.shoop_c_handle:
            destroy_audio_channel(self.shoop_c_handle)
            self.shoop_c_handle = None

    def __del__(self):
        self.destroy()
    
class BackendLoopMidiChannel:
    def __init__(self, loop : 'BackendLoop', c_handle : 'POINTER(shoopdaloop_loop_midi_channel_t)'):
        self.loop_shoop_c_handle = loop.c_handle()
        self.shoop_c_handle = c_handle
    
    def get_data(self):
        r = get_midi_channel_data(self.shoop_c_handle)
        msgs = [backend_midi_message_to_dict(r[0].events[i][0]) for i in range(r[0].n_events)]
        destroy_midi_channel_data(r)
        return msgs
    
    def load_data(self, msgs):
        d = alloc_midi_channel_data(len(msgs))
        d[0].length_samples = msgs[len(msgs)-1]['time'] + 1
        for idx, m in enumerate(msgs):
            d[0].events[idx] = midi_message_dict_to_backend(m)
        load_midi_channel_data(self.shoop_c_handle, d)
        destroy_midi_channel_data(d)
    
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
    
    def destroy(self):
        if self.shoop_c_handle:
            destroy_midi_channel(self.shoop_c_handle)
            self.shoop_c_handle = None

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
        print('transition: {}, {}, {}'.format(to_state, cycles_delay, wait_for_sync))
        loop_transition(self.shoop_c_handle,
                                to_state.value,
                                cycles_delay,
                                wait_for_sync)
    
    # Static version for multiple loops
    def transition_multiple(loops, to_state : Type['LoopMode'],
                   cycles_delay : int, wait_for_sync : bool):
        print('transition {} loops: {}, {}, {}'.format(len(loops), to_state, cycles_delay, wait_for_sync))
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
    
    def set_passthrough_volume(self, passthrough_volume):
        if self._c_handle:
            set_audio_port_passthroughVolume(self._c_handle, passthrough_volume)
    
    def set_muted(self, muted):
        if self._c_handle:
            set_audio_port_muted(self._c_handle, (1 if muted else 0))
    
    def set_passthrough_muted(self, muted):
        if self._c_handle:
            set_audio_port_passthroughMuted(self._c_handle, (1 if muted else 0))
    
    def connect_passthrough(self, other):
        add_audio_port_passthrough(self._c_handle, other.c_handle())

    def __del__(self):
        print("BACKEND AUDIO PORT DESTROY BY __del__: {}".format(self))
        raise Exception("Nope")
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
    
    def get_state(self):
        state = get_fx_chain_state(self._c_handle)
        rval = FXChainState(state[0])
        destroy_fx_chain_state(state)
        return rval

class Backend:
    def __init__(self, c_handle : 'POINTER(shoopdaloop_backend_instance_t)'):
        self._c_handle = c_handle

    def get_pyjacklib_client_handle(self):
        return cast(get_jack_client_handle(self._c_handle), POINTER(jacklib.jack_client_t))

    def maybe_shoop_jack_client_handle(self):
        return maybe_jack_client_handle(self._c_handle)

    def get_client_name(self):
        return str(get_jack_client_name(self._c_handle).decode('ascii'))

    def get_sample_rate(self):
        return int(get_sample_rate(self._c_handle))

    def create_loop(self) -> Type['BackendLoop']:
        handle = create_loop(self._c_handle)
        rval = BackendLoop(handle)
        return rval
    
    def create_fx_chain(self, chain_type : Type['FXChainType']) -> Type['BackendFXChain']:
        handle = create_fx_chain(self._c_handle, chain_type.value)
        rval = BackendFXChain(handle, chain_type)
        return rval

    def open_jack_audio_port(self, name_hint : str, direction : int) -> 'BackendAudioPort':
        _dir = (Input if direction == PortDirection.Input.value else Output)
        handle = open_jack_audio_port(self._c_handle, name_hint.encode('ascii'), _dir)
        port = BackendAudioPort(handle, direction)
        return port

    def open_jack_midi_port(self, name_hint : str, direction : int) -> 'BackendMidiPort':
        _dir = (Input if direction == PortDirection.Input.value else Output)
        handle = open_jack_midi_port(self._c_handle, name_hint.encode('ascii'), _dir)
        port = BackendMidiPort(handle, direction)
        return port
    
    def _fx_chain_ports_get(self, get_fn, fx_chain, idx):
        n_ports = c_uint()
        ports = get_fn(fx_chain.c_handle(), byref(n_ports))
        if idx >= n_ports.value:
            raise Exception('Trying to get port {} of {} ports'.format(idx, int(n_ports)))
        return ports[idx]
    
    def get_fx_chain_audio_input_port(self, fx_chain : Type['BackendFXChain'], idx : int):
        return BackendAudioPort(self._fx_chain_ports_get(fx_chain_audio_input_ports, fx_chain, idx), PortDirection.Output.value)
    
    def get_fx_chain_audio_output_port(self, fx_chain : Type['BackendFXChain'], idx : int):
        return BackendAudioPort(self._fx_chain_ports_get(fx_chain_audio_output_ports, fx_chain, idx), PortDirection.Input.value)
    
    def get_fx_chain_midi_input_port(self, fx_chain : Type['BackendFXChain'], idx : int):
        return BackendMidiPort(self._fx_chain_ports_get(fx_chain_midi_input_ports, fx_chain, idx), PortDirection.Output.value)
        
    def get_sample_rate(self):
        return int(get_sample_rate(self._c_handle))

    def terminate(self):
        terminate(self._c_handle)

def init_backend(backend_type : Type[BackendType], client_name_hint : str):
    _ptr = initialize(backend_type.value, client_name_hint.encode('ascii'))
    b = Backend(_ptr)
    return b