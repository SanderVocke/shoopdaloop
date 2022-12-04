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
    SetLoopVolume = backend.SetLoopVolume
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

class PortActionType(Enum):
    DoMute = backend.DoMute
    DoMuteInput = backend.DoMuteInput
    DoUnmute = backend.DoUnmute
    DoUnmuteInput = backend.DoUnmuteInput
    SetPortVolume = backend.SetPortVolume
    SetPortPassthrough = backend.SetPortPassthrough

class MIDIMessageFilterType(Enum):
    Channel = 0
    IsNoteKind = 1 # filter on note messages
    NoteId = 2
    NoteVelocity = 3
    IsNoteOn = 4
    IsNoteOff = 5
    IsNoteShortPress = 6
    IsNoteDoublePress = 7
    IsCCKind = 8 # filter on CC messages
    CCController = 9
    CCValue = 10

    names = {
        Channel: 'channel',
        IsNoteKind: 'note_kind',
        NoteId: 'note_id',
        NoteVelocity: 'note_velocity',
        IsNoteOn: 'is_note_on',
        IsNoteOff: 'is_note_off',
        IsNoteShortPress: 'is_note_short_press',
        IsNoteDoublePress: 'is_note_double_press',
        IsCCKind: 'is_cc_kind',
        CCController: 'cc_controller',
        CCValue: 'cc_value',
    }