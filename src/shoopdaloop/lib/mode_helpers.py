from .backend_wrappers import *

def is_playing_mode(mode):
    return mode in [LoopMode.Playing.value, LoopMode.PlayingDryThroughWet.value]

def is_recording_mode(mode):
    return mode in [LoopMode.Recording.value, LoopMode.RecordingDryIntoWet.value]

def is_running_mode(mode):
    return mode in [
        LoopMode.Playing.value,
        LoopMode.Replacing.value,
        LoopMode.Recording.value,
        LoopMode.RecordingDryIntoWet.value,
        LoopMode.PlayingDryThroughWet.value
    ]
