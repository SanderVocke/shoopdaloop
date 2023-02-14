import sys
sys.path.append('../..')
import build.backend.frontend_interface.shoopdaloop_backend as backend
from typing import *
from third_party.pyjacklib import jacklib
from ctypes import *
import mido

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
    def __init__(self, loop : Type[BackendLoop]):
        self.loop_shoop_c_handle = loop.c_handle()
        self.shoop_c_handle = backend.add_audio_channel(loop.c_handle())
    
    def get_data(self) -> list[float]:
        r = backend.get_audio_channel_data(self.shoop_c_handle)
        data = [float(r.data[i]) for i in range(r.n_samples)]
        backend.shoopdaloop_free(r.data)
        return data

class BackendLoopMidiChannel:
    def __init__(self, loop : Type[BackendLoop]):
        self.shoop_c_handle = backend.add_midi_channel(loop.c_handle())
    
    def get_data(self) -> list(BackendMidiMessage):
        r = backend.get_midi_channel_data(self.shoop_c_handle)
        for idx in range(r.n_events):
            

class BackendLoop:
    def __init__(self, backend : Type[Backend]):
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
    
    def __del__(self):
        backend.delete_loop(self.shoop_c_handle)

class Backend:
    def __init__(self, client_name_hint : str):
        backend.initialize(client_name_hint.encode('ascii'))
        self.shoop_c_handle = backend.get_jack_client_handle()
        self.jack_c_handle = cast(shoop_c_handle, POINTER(jacklib.jack_client_t))
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
    
    def __del__(self):
        backend.terminate()


    


