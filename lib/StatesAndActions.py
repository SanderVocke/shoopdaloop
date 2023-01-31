from enum import Enum
import sys, os
pwd = os.path.dirname(__file__)
sys.path.append(pwd + '/..')
sys.path.append(pwd + '/../build')

from backend.frontend_interface import shoopdaloop_backend as backend

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
    DoReRecordFX = -2
    DoPlaySoloInTrack = -3
    DoTogglePlaying = -4 # Toggles between playing/stopped
    DoSelect = -5        # Highlights (a) loop(s) for operating on
    DoTarget = -6        # Selects the targeted loop

    names = {
        DoPlay: 'play',
        DoPlayMuted: 'playMuted',
        DoPlaySoloInTrack: 'playSoloInTrack',
        DoStop: 'stop',
        DoRecord: 'record',
        DoRecordNCycles: 'recordN',
        DoReRecordFX: 'reRecordFX',
        DoPlayLiveFX: 'playLiveFX',
        DoClear: 'clear',
        SetLoopVolume: 'set_volume',
        DoTogglePlaying: 'toggle_playing',
        DoSelect: 'select',
        DoTarget: 'target'
    }

class PortActionType(Enum):
    DoMute = backend.DoMute
    DoMuteInput = backend.DoMuteInput
    DoUnmute = backend.DoUnmute
    DoUnmuteInput = backend.DoUnmuteInput
    SetPortVolume = backend.SetPortVolume
    SetPortPassthrough = backend.SetPortPassthrough

    names = {
        DoMute: 'mute',
        DoMuteInput: 'muteInput',
        DoUnmute: 'unmute',
        DoUnmuteInput: 'unmuteInput',
        SetPortVolume: 'setVolume',
        SetPortPassthrough: 'setPassthrough'
    }

class MIDIMessageFilterType(Enum):
    IsNoteKind = 0 # filter on note messages
    IsNoteOn = 1
    IsNoteOff = 2
    IsNoteShortPress = 3
    IsNoteDoublePress = 4
    IsCCKind = 5 # filter on CC messages

    names = {
        IsNoteKind: 'note_kind',
        IsNoteOn: 'is_note_on',
        IsNoteOff: 'is_note_off',
        IsNoteShortPress: 'is_note_short_press',
        IsNoteDoublePress: 'is_note_double_press',
        IsCCKind: 'is_cc_kind',
    }