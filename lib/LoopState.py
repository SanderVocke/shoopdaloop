from enum import Enum

class LoopState(Enum):
    Unknown = -1
    Off = 0
    WaitStart = 1
    Recording = 2
    WaitStop = 3
    Playing = 4
    Overdubbing = 5
    Multiplying = 6
    Inserting = 7
    Replacing = 8
    Delay = 9
    Muted = 10
    Scratching = 11
    OneShot = 12
    Substitute = 13
    Paused = 14
    # Extended states for FX loops
    RecordingWet = 15
    PlayingDryLiveWet = 16
    PlayingDry = 17