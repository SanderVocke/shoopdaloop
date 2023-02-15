from .StatesAndActions import *

def is_playing_state(mode):
            return mode in \
                [LoopMode.Playing.value, LoopMode.PlayingMuted.value, LoopMode.PlayingLiveFX.value]