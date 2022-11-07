from enum import Enum
import sys, os
pwd = os.path.dirname(__file__)
sys.path.append(pwd + '/..')
sys.path.append(pwd + '/../build')

from backend import shoopdaloop_backend as backend

class LoopState(Enum):
    Stopped = backend.Stopped
    Playing = backend.Playing
    PlayingMuted = backend.PlayingMuted
    Recording = backend.Recording
    # Extended states for front-end
    Unknown = -1
    RecordingFX = -2
    PlayingLiveFX = -3
    Empty = -4

    names = {
        Stopped: 'Stopped',
        Playing: 'Playing',
        PlayingMuted: 'PlayingMuted',
        Recording: 'Recording',
        Unknown: 'Unknown',
        RecordingFX: 'Re-recording FX',
        PlayingLiveFX: 'Playing with live FX',
        Empty: 'Empty',
    }

class LoopActionType(Enum):
    DoPlay = backend.DoPlay
    DoPlayMuted = backend.DoPlayMuted
    DoStop = backend.DoStop
    DoRecord = backend.DoRecord
    DoRecordNCycles = backend.DoRecordNCycles
    DoClear = backend.DoClear
    # Extended actions for front-end
    DoPlayLiveFX = -1
    DoRecordFX = -2

    names = {
        DoPlay: 'play',
        DoPlayMuted: 'playMuted',
        DoStop: 'stop',
        DoRecord: 'record',
        DoRecordFX: 'recordFX',
        DoPlayLiveFX: 'playLiveFX'
    }