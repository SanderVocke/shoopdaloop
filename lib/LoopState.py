from enum import Enum
import sys
sys.path.append('..')
import build.backend.shoopdaloop_backend as backend

class LoopState(Enum):
    Stopped = backend.Stopped
    Playing = backend.Playing
    Recording = backend.Recording
    # Extended states for front-end
    Unknown = -1
    RecordingFX = -2
    PlayingLiveFX = -3

    names = {
        Stopped: 'Stopped',
        Playing: 'Playing',
        Recording: 'Recording',
        Unknown: 'Unknown',
        RecordingFX: 'Re-recording FX',
        PlayingLiveFX: 'Playing with live FX',
    }

class LoopActionType(Enum):
    DoPlay = backend.DoPlay
    DoStop = backend.DoStop
    DoRecord = backend.DoRecord
    # Extended actions for front-end
    DoPlayLiveFX = -1
    DoRecordFX = -2

    names = {
        DoPlay: 'play',
        DoStop: 'stop',
        DoRecord: 'record',
        DoRecordFX: 'recordFX',
        DoPlayLiveFX: 'playLiveFX'
    }