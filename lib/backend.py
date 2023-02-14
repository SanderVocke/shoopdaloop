import sys
import os
sys.path.append(
    os.path.dirname(os.path.realpath(__file__)) + '/..'
)
sys.path.append(
    os.path.dirname(os.path.realpath(__file__)) + '/../build/backend/frontend_interface'
)
print(sys.path)
import shoopdaloop_backend as backend
from typing import *
from third_party.pyjacklib import jacklib
from ctypes import *
import mido
from enum import Enum
from dataclasses import dataclass

class PortDirection(Enum):
    Input = 0
    Output = 1

class PortType(Enum):
    Audio = 0
    Midi = 1

class LoopState(Enum):
    Stopped = backend.Stopped
    Playing = backend.Playing
    PlayingMuted = backend.PlayingMuted
    Recording = backend.Recording

# Wraps the Shoopdaloop backend with more Python-friendly wrappers
@dataclass
class BackendMidiMessage:
    time: int
    data: bytes

def from_midi_event_t(msg) -> BackendMidiMessage:
    time = msg.time
    size = msg.size
    array_type = (c_ubyte * size)
    array = cast(msg.data, array_type)
    data = bytes(array)
    return BackendMidiMessage(time, data)

class BackendLoopAudioChannel:
    def __init__(self, loop):
        self.loop_shoop_c_handle = loop.c_handle()
        self.shoop_c_handle = backend.add_audio_channel(loop.c_handle())
    
    def get_data(self) -> list[float]:
        r = backend.get_audio_channel_data(self.shoop_c_handle)
        data = [float(r.data[i]) for i in range(r.n_samples)]
        backend.free_audio_channel_data(r)
        return data
    
    def connect(self, port):
        if port.direction() == PortDirection.Input:
            backend.connect_audio_input(self.shoop_c_handle, port.c_handle())
        else:
            backend.connect_audio_output(self.shoop_c_handle, port.c_handle())

class BackendLoopMidiChannel:
    def __init__(self, loop):
        self.shoop_c_handle = backend.add_midi_channel(loop.c_handle())
    
    def get_data(self) -> list[BackendMidiMessage]:
        r = backend.get_midi_channel_data(self.shoop_c_handle)
        msgs = [from_midi_event_t(r.events[i]) for i in range(r.n_events)]
        backend.free_midi_channel_data(r)
        return msgs
    
    def connect(self, port):
        if port.direction() == PortDirection.Input:
            backend.connect_midi_input(self.shoop_c_handle, port.c_handle())
        else:
            backend.connect_midi_output(self.shoop_c_handle, port.c_handle())

class BackendLoop:
    def __init__(self, _backend):
        self.shoop_c_handle = backend.create_loop()
        self.audio_channels = []
        self.midi_channels = []
    
    def c_handle(self):
        return self.shoop_c_handle
    
    def add_audio_channel(self) -> Type[BackendLoopAudioChannel]:
        rval = BackendLoopAudioChannel(self)
        self.audio_channels.append(rval)
        return rval
    
    def add_midi_channel(self) -> Type[BackendLoopMidiChannel]:
        rval = BackendLoopMidiChannel(self)
        self.midi_channels.append(rval)
        return rval
    
    def audio_channels(self) -> list[Type[BackendLoopAudioChannel]]:
        return self.audio_channels
    
    def midi_channels(self) -> list[Type[BackendLoopMidiChannel]]:
        return self.midi_channels

    def transition(self, to_state : Type[LoopState],
                   cycles_delay : int, wait_for_soft_sync : bool):
        backend.loop_transition(self.shoop_c_handle,
                                to_state.value,
                                cycles_delay,
                                wait_for_soft_sync)
    
    def __del__(self):
        backend.delete_loop(self.shoop_c_handle)

class BackendPort:
    def __init__(self, _backend, port_type : Type[PortType],
                 name_hint : str, direction : Type[PortDirection]):
        self._direction = direction
        self._port_type = port_type
        _dir = (backend.Input if direction == PortDirection.Input else backend.Output)
        if port_type == PortType.Audio:
            self._c_handle = backend.open_audio_port(name_hint.encode('ascii'), _dir)
        elif port_type == PortType.Midi:
            self._c_handle = backend.open_midi_port(name_hint.encode('ascii'), _dir)
    
    def port_type(self) -> Type[PortType]:
        return self._port_type
    
    def direction(self) -> Type[PortDirection]:
        return self._direction
    
    def c_handle(self):
        return self._c_handle


class Backend:
    def __init__(self, client_name_hint : str):
        backend.initialize(client_name_hint.encode('ascii'))
        self.shoop_c_handle = backend.get_jack_client_handle()
        self.jack_c_handle = cast(self.shoop_c_handle, POINTER(jacklib.jack_client_t))
        self.loops = []

    def pyjacklib_client_handle(self):
        return self.jack_c_handle
    
    def client_name(self):
        return str(backend.get_jack_client_name().decode('ascii'))
    
    def sample_rate(self):
        return int(backend.get_sample_rate())
    
    def create_loop(self) -> Type[BackendLoop]:
        rval = BackendLoop(self)
        self.loops.append(rval)
        return rval
    
    def open_port(self, name_hint : str, _type : Type[PortType], direction : Type[PortDirection]):
        return BackendPort(self, _type, name_hint, direction)
    
    def __del__(self):
        backend.terminate()