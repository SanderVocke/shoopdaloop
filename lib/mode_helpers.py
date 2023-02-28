from .backend_interface import *

def is_playing_mode(mode):
    return mode in [LoopMode.Playing.value, LoopMode.PlayingDryThroughWet.value]