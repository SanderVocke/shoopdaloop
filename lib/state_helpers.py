from .StatesAndActions import *

def is_playing_state(state):
            return state in \
                [LoopState.Playing.value, LoopState.PlayingMuted.value, LoopState.PlayingLiveFX.value]