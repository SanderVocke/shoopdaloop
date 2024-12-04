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

class PyAudioDriverType(Enum):
    Jack = bindings.Jack
    JackTest = bindings.JackTest
    Dummy = bindings.Dummy

DontWaitForSync = -1
DontAlignToSyncImmediately = -1

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
