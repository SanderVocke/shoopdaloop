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

import shoop_py_backend
from shoopdaloop.lib.init_dynlibs import init_dynlibs
init_dynlibs()

intmax = 2**31-1
intmin = -intmax - 1
def to_int(val):
    v = int(val)
    return max(min(v, intmax), intmin)

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
