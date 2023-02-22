import sys
import os
sys.path.append(
    os.path.dirname(os.path.realpath(__file__)) + '/..'
)
sys.path.append(
    os.path.dirname(os.path.realpath(__file__)) + '/../build/backend/frontend_interface'
)
import shoopdaloop_backend as backend
from typing import *
from third_party.pyjacklib import jacklib
from ctypes import *
import mido
from enum import Enum
from dataclasses import dataclass
import typing
import copy

class PortDirection(Enum):
    Input = 0
    Output = 1

class LoopMode(Enum):
    Unknown = backend.Unknown
    Stopped = backend.Stopped
    Playing = backend.Playing
    Recording = backend.Recording
    PlayingDryThroughWet = backend.PlayingDryThroughWet
    RecordingDryIntoWet = backend.RecordingDryIntoWet

class ChannelMode(Enum):
    Disabled = backend.Disabled
    Direct = backend.Direct
    Dry = backend.Dry
    Wet = backend.Wet

@dataclass
class LoopAudioChannelState:
    mode : Type[ChannelMode]
    output_peak : float
    volume : float

    def __init__(self, backend_state : 'backend.loop_audio_channel_state_t'):
        self.output_peak = backend_state.output_peak
        self.volume = backend_state.volume
        self.mode = backend_state.mode

@dataclass
class LoopMidiChannelState:
    mode: Type[ChannelMode]
    n_events_triggered : int

    def __init__(self, backend_state : 'backend.loop_midi_channel_state_t'):
        self.n_events_triggered = backend_state.n_events_triggered
        self.mode = backend_state.mode

@dataclass
class LoopState:
    length: int
    position: int
    mode: Type[LoopMode]
    maybe_next_mode: typing.Union[Type[LoopMode], None]
    maybe_next_delay: typing.Union[int, None]

    def __init__(self, backend_loop_state : 'backend.loop_state_t'):
        self.length = backend_loop_state.length
        self.position = backend_loop_state.position
        self.mode = backend_loop_state.mode
        self.maybe_next_mode =  (None if backend_loop_state.maybe_next_mode == backend.LOOP_MODE_INVALID else backend_loop_state.maybe_next_mode)
        self.maybe_next_delay = (None if backend_loop_state.maybe_next_mode == backend.LOOP_MODE_INVALID else backend_loop_state.maybe_next_mode_delay)
    
# Wraps the Shoopdaloop backend with more Python-friendly wrappers
@dataclass
class BackendMidiMessage:
    time: int
    data: bytes

    def __init__(self, backend_msg : 'backend.midi_event_t'):
        time = msg.time
        size = msg.size
        array_type = (c_ubyte * size)
        array = cast(msg.data, array_type)
        data = bytes(array)
        self.time = time
        self.data = data

class BackendLoopAudioChannel:
    def __init__(self, loop, c_handle : 'POINTER(backend.shoopdaloop_loop_audio_channel)'):
        self.loop_shoop_c_handle = loop.c_handle()
        self.shoop_c_handle = c_handle
    
    def get_data(self) -> list[float]:
        r = backend.get_audio_channel_data(self.shoop_c_handle)
        data = [float(r[0].data[i]) for i in range(r[0].n_samples)]
        backend.free_audio_channel_data(r)
        return data
    
    def connect(self, port : 'BackendAudioPort'):
        if port.direction() == PortDirection.Input:
            backend.connect_audio_input(self.shoop_c_handle, port.c_handle())
        else:
            backend.connect_audio_output(self.shoop_c_handle, port.c_handle())
    
    def get_rms_data(self, from_sample, to_sample, samples_per_bin):
        data = backend.get_audio_rms_data (self.shoop_c_handle, from_sample, to_sample, samples_per_bin)
        result = [float(data[0].data[i]) for i in range(data[0].n_samples)]
        array = None
        backend.destroy_audio_channel_data(data)
        return result
    
    def load_data(self, data):
        backend_data = backend.alloc_audio_channel_data(len(data))
        print(data)
        for i in range(len(data)):
            backend_data[0].data[i] = data[i]
        backend.load_audio_channel_data(self.shoop_c_handle, backend_data)
        backend.destroy_audio_channel_data(backend_data)
    
    def get_state(self):
        state = backend.get_audio_channel_state(self.shoop_c_handle)
        rval = LoopAudioChannelState(state[0])
        backend.destroy_audio_channel_state_info(state)
        return rval
    
    def set_mode(self, mode : Type['ChannelMode']):
        backend.set_audio_channel_mode(self.shoop_c_handle, mode.value)
    
    def destroy(self):
        if self.shoop_c_handle:
            backend.destroy_audio_channel(self.shoop_c_handle)
            self.shoop_c_handle = None

    def __del__(self):
        self.destroy()
    
class BackendLoopMidiChannel:
    def __init__(self, loop : 'BackendLoop', c_handle : 'POINTER(backend.shoopdaloop_loop_midi_channel_t)'):
        self.loop_shoop_c_handle = loop.c_handle()
        self.shoop_c_handle = c_handle
    
    def get_data(self) -> list['BackendMidiMessage']:
        r = backend.get_midi_channel_data(self.shoop_c_handle)
        msgs = [BackendMidiMessage(r[0].events[i]) for i in range(r[0].n_events)]
        backend.destroy_midi_channel_data(r)
        return msgs
    
    def connect(self, port : 'BackendMidiPort'):
        if port.direction() == PortDirection.Input:
            backend.connect_midi_input(self.shoop_c_handle, port.c_handle())
        else:
            backend.connect_midi_output(self.shoop_c_handle, port.c_handle())
    
    def get_state(self):
        state = backend.get_midi_channel_state(self.shoop_c_handle)
        rval = LoopMidiChannelState(state[0])
        backend.destroy_midi_channel_state_info(state)
        return rval
    
    def set_mode(self, mode : Type['ChannelMode']):
        backend.set_midi_channel_mode(self.shoop_c_handle, mode.value)
    
    def destroy(self):
        if self.shoop_c_handle:
            backend.destroy_audio_channel(self.shoop_c_handle)
            self.shoop_c_handle = None

    def __del__(self):
        self.destroy()

class BackendLoop:
    def __init__(self, c_handle : 'POINTER(backend.shoopdaloop_loop_t)'):
        self.shoop_c_handle = c_handle
    
    def c_handle(self):
        return self.shoop_c_handle
    
    def add_audio_channel(self, mode : Type['ChannelMode']) -> 'BackendLoopAudioChannel':
        rval = BackendLoopAudioChannel(self, backend.add_audio_channel(self.c_handle(), mode.value))
        return rval
    
    def add_midi_channel(self, mode : Type['ChannelMode']) -> 'BackendLoopMidiChannel':
        rval = BackendLoopMidiChannel(self, backend.add_midi_channel(self.c_handle(), mode.value))
        return rval

    def transition(self, to_state : Type['LoopMode'],
                   cycles_delay : int, wait_for_sync : bool):
        print('transition: {}, {}, {}'.format(to_state, cycles_delay, wait_for_sync))
        backend.loop_transition(self.shoop_c_handle,
                                to_state.value,
                                cycles_delay,
                                wait_for_sync)
    
    def get_state(self):
        state = backend.get_loop_state(self.shoop_c_handle)
        rval = LoopState(state[0])
        backend.destroy_loop_state_info(state)
        return rval
    
    def set_length(self, length):
        backend.set_loop_length(self.shoop_c_handle, length)
    
    def set_position(self, position):
        backend.set_loop_position(self.shoop_c_handle, position)
    
    def clear(self, length):
        backend.clear_loop(self.shoop_c_handle, length)
    
    def set_sync_source(self, loop):
        if loop:
            backend.set_loop_sync_source(self.shoop_c_handle, loop.shoop_c_handle)
        else:
            backend.set_loop_sync_source(self.shoop_c_handle, None)
    
    def __del__(self):
        backend.destroy_loop(self.shoop_c_handle)

class BackendAudioPort:
    def __init__(self, c_handle : 'POINTER(backend.shoopdaloop_audio_port_t)',
                 direction : 'PortDirection'):
        self._direction = direction
        self._c_handle = c_handle
    
    def direction(self) -> Type['PortDirection']:
        return self._direction
    
    def c_handle(self):
        return self._c_handle
    
    def name(self):
        state = backend.get_audio_port_state(self._c_handle)
        name = str(state[0].name.decode('ascii'))
        backend.destroy_audio_port_state(state)
        return name
    
    def __del__(self):
        backend.destroy_audio_port(self._c_handle)

class BackendMidiPort:
    def __init__(self, c_handle : 'POINTER(backend.shoopdaloop_midi_port_t)',
                 direction : 'PortDirection'):
        self._direction = direction
        self._c_handle = c_handle
    
    def direction(self) -> Type['PortDirection']:
        return self._direction
    
    def c_handle(self):
        return self._c_handle
    
    def name(self):
        state = backend.get_audio_port_state(self._c_handle)
        name = str(state[0].name.decode('ascii'))
        backend.destroy_midi_port_state(state[0])
        return name
    
    def __del__(self):
        backend.destroy_midi_port(self._c_handle)

def init_backend(client_name_hint : str):
    backend.initialize(client_name_hint.encode('ascii'))

def get_pyjacklib_client_handle():
    return cast(backend.get_jack_client_handle(), POINTER(jacklib.jack_client_t))

def get_shoop_jack_client_handle():
    return backend.get_jack_client_handle()

def get_client_name():
    return str(backend.get_jack_client_name().decode('ascii'))

def get_sample_rate():
    return int(backend.get_sample_rate())

def create_loop() -> Type['BackendLoop']:
    handle = backend.create_loop()
    rval = BackendLoop(handle)
    return rval

def open_audio_port(name_hint : str, direction : 'PortDirection') -> 'BackendAudioPort':
    _dir = (backend.Input if direction == PortDirection.Input else backend.Output)
    handle = backend.open_audio_port(name_hint.encode('ascii'), _dir)
    port = BackendAudioPort(handle, direction)
    return port

def open_midi_port(name_hint : str, direction : 'PortDirection') -> 'BackendMidiPort':
    _dir = (backend.Input if direction == PortDirection.Input else backend.Output)
    handle = backend.open_midi_port(name_hint.encode('ascii'), _dir)
    port = BackendMidiPort(handle, direction)
    return port

def get_sample_rate():
    return int(backend.get_sample_rate())

def terminate():
    backend.terminate()