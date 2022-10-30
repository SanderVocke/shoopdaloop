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

class LoopActionType(Enum):
    Activate = 0
    Play = 1
    Pause = 2
    Mute = 3
    Unmute = 4
    Record = 5
    RecordNCycles = 6 # Requires arg n_cycles
    RecordFX = 7
    PlayLiveFX = 8
    SetVolume = 9 # Requires arg  value (0.0 .. 1.0)
    SetPan = 10 # Requires arg value (-1.0 .. 1.0)
    Play_Pause = 11
    Mute_Unmute = 12

loop_action_names = {
    'activate' : LoopActionType.Activate.value,
    'play' : LoopActionType.Play.value,
    'pause' : LoopActionType.Pause.value,
    'mute' : LoopActionType.Mute.value,
    'unmute' : LoopActionType.Unmute.value,
    'record' : LoopActionType.Record.value,
    'recordN' : LoopActionType.RecordNCycles.value,
    'recordFX' : LoopActionType.RecordFX.value,
    'playFX' : LoopActionType.PlayLiveFX.value,
    'setVolume' : LoopActionType.SetVolume.value,
    'setPan' : LoopActionType.SetPan.value,
    'playPause' : LoopActionType.Play_Pause.value,
    'muteUnmute' : LoopActionType.Mute_Unmute.value,
}