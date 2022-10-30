from PyQt6.QtCore import QObject, pyqtSignal, pyqtSlot

from ..LoopState import LoopState
from ..MidiScripting import *
from ..flatten import flatten

from enum import Enum
from typing import Type
import time

class MessageFilterType(Enum):
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

class InputRule:
    press_period = 0.5

    def __init__(self,
                 filters : dict[Type[MessageFilterType], Union[None, int]],
                 formula : str,
                 ):
        self.filters = filters
        self.formula = formula
        # internal
        self._lastOn = {}
        self._lastOff = {}
        self._lastPress = {}
    
    def apply_filters(msg_bytes):
        if len(msg_bytes) == 0:
            return False

        type_byte = msg_bytes[0] & 0xF0
        channel = msg_bytes[0] & 0x0F
        controller_or_note = None
        value_or_velocity = None
        filters = self.filters

        if len(msg_bytes) >= 2:
            controller_or_note = msg_bytes[1]
        
        if len(msg_bytes) >= 3:
            value_or_velocity = msg_bytes[2]
        
        if (MessageFilterType.CCController in filters or MessageFilterType.CCValue in filters) and \
           MessageFilterType.IsCCKind not in filters:
           filters[MessageFilterType.IsCCKind] = None
        
        if (MessageFilterType.NoteId in filters or \
            MessageFilterType.NoteVelocity in filters or \
            MessageFilterType.IsNoteOff in filters or \
            MessageFilterType.IsNoteOn in filters or \
            MessageFilterType.IsNoteShortPress or \
            MessageFilterType.IsNoteDoublePress) and \
           MessageFilterType.IsNoteKind not in filters:
           filters[MessageFilterType.IsNoteKind] = None
        
        if type_byte == 0x80:
            print('noteOff {} {}'.format(controller_or_note, value_or_velocity))
            if MessageFilterType.IsCCKind in filters or \
                MessageFilterType.IsNoteOn in filters or \
                (MessageFilterType.NoteId in filters and filters[MessageFilterType.NoteId] != controller_or_note) or \
                (MessageFilterType.NoteVelocity in filters and filters[MessageFilterType.NoteVelocity] != value_or_velocity):
                return False
               
        elif type_byte == 0x90:
            print('noteOn {}'.format(controller_or_note, value_or_velocity))
            if MessageFilterType.IsCCKind in filters or \
                MessageFilterType.IsNoteOff in filters or \
                (MessageFilterType.NoteId in filters and filters[MessageFilterType.NoteId] != controller_or_note) or \
                (MessageFilterType.NoteVelocity in filters and filters[MessageFilterType.NoteVelocity] != value_or_velocity):
                return False

        elif type_byte == 0xB0:
            print('CC {} {}'.format(controller_or_note, value_or_velocity))
            if MessageFilterType.IsNoteKind in filters or \
                (MessageFilterType.CCController in filters and filters[MessageFilterType.CCController] != controller_or_note) or \
                (MessageFilterType.CCValue in filters and filters[MessageFilterType.CCValue] != value_or_velocity):
                return False
        
        # TODO: presses and double-presses
        
        return True

def get_builtin_substitutions(msg_bytes):

    if len(msg_bytes) < 3:
        return {}
    
    type_byte = msg_bytes[0] & 0xF0
    channel = msg_bytes[0] & 0x0F

    if type_byte == 0x90:
        return {
            'type': 'noteOn',
            'note': msg_bytes[1],
            'velocity': msg_bytes[2],
            'channel': channel
        }
    elif type_byte == 0x80:
        return {
            'type': 'noteOff',
            'note': msg_bytes[1],
            'velocity': msg_bytes[2],
            'channel': channel
        }
    elif type_byte == 0xB0:
        return {
            'type': 'cc',
            'controller': msg_bytes[1],
            'value': msg_bytes[2],
            'channel': channel
        }
    
    return {}

# For communicating with MIDI control/input devices.
class MIDIControlManager(QObject):
    def __init__(self, parent=None):
        super(MIDIControlManager, self).__init__(parent)
        self._loop_state_formulas = {} # Loop state value to formula
        self._loop_state_default_formula = None # Used if there is no formula for a given state
        self._scripting_section_formula = None
        self._active_scene_formula = None
        self._substitution_rules = {}
        self._input_rules = []

        self._loop_states_cache = {} # tuple of (track, loop) => last transmitted state
        self._scripting_section_cache = None # last transmitted section
        self._active_scene_cache = None # last transmitted active scene

        # TODO: hard-coded APC Mini for now
        self._substitution_rules['loop_note'] = '0 if (track == 0 and loop == 0) else (56+track-1-loop*8)'
        self._loop_state_default_formula = 'noteOn(0, loop_note, 5)'
        self._loop_state_formulas[LoopState.Recording.value] = 'noteOn(0, loop_note, 3)'
        self._loop_state_formulas[LoopState.Playing.value] = 'noteOn(0, loop_note, 1)'
        self._loop_state_formulas[LoopState.WaitStart.value] = 'noteOn(0, loop_note, 4)'
        self._loop_state_formulas[LoopState.WaitStop.value] = self._loop_state_formulas[LoopState.WaitStart.value]
        self._loop_state_formulas[LoopState.Off.value] = 'noteOn(0, loop_note, 0)'
        self._substitution_rules['note_track'] = '0 if note == 0 else (note % 8 + 1)'
        self._substitution_rules['note_loop'] = '0 if note == 0 else 7 - (note / 8)'
        self._input_rules = [
            InputRule({
                MessageFilterType.IsNoteDoublePress: None,
            }, 'loopAction(note_track, note_loop, playPause) if note <= 64')
        ]

    sendMidi = pyqtSignal(list) # list of byte values
    loopAction = pyqtSignal(int, int, int, list) # track idx, loop idx, LoopActionType value, args

    def send_midi_messages(self, messages):
        for msg in messages:
            if isinstance(msg, MIDIMessage):
                self.sendMidi.emit(msg.bytes())
    
    def trigger_actions(self, actions):
        for action in actions:
            if isinstance(action, LoopAction):
                self.loopAction.emit(action.track_idx, action.loop_idx, action.action_type.value, action.args)

    
    @pyqtSlot(list)
    def receiveMidi(self, _bytes):
        if len(_bytes < 1):
            return
        formulas_to_execute = [r.formula for r in self._input_rules if r.apply_filters(_bytes)]
        substitutions = get_builtin_substitutions(_bytes)
        actions = flatten([eval_formula(f, substitutions) for f in formulas_to_execute])
        self.trigger_actions(actions)
    
    @pyqtSlot(int, int, int)
    def loop_state_changed(self, track, index, state):
        if (track, index) in self._loop_states_cache and self._loop_states_cache[(track, index)] == state:
            return # already sent this
        
        if state in self._loop_state_formulas:
            self.send_midi_messages(eval_formula(self._loop_state_formulas[state], {**{'track': track, 'loop': index, 'state': state}, **self._substitution_rules}))
            self._loop_states_cache[(track, index)] = state
        elif self._loop_state_default_formula:
            self.send_midi_messages(eval_formula(self._loop_state_default_formula, {**{'track': track, 'loop': index, 'state': state}, **self._substitution_rules}))
            self._loop_states_cache[(track, index)] = state

    @pyqtSlot(int)
    def active_sripting_section_changed(self, idx):
        if self._scripting_section_cache == idx:
            return # already sent this

        if self._scripting_section_formula:
            self.send_midi_messages(eval_formula(self._scripting_section_formula, {'section': idx}))
            self._scripting_section_cache = idx
    
    @pyqtSlot(int)
    def active_scene_changed(self, idx):
        if self._active_scene_cache == idx:
            return # already sent this
        
        if self._active_scene_formula:
            self.send_midi_messages(eval_formula(self._active_scene_formula, {'scene': idx}))
            self._active_scene_cache = idx