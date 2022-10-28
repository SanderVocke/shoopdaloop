from PyQt6.QtCore import QObject, pyqtSignal, pyqtSlot

from ..LoopState import LoopState
from ..MidiScripting import *

from enum import Enum
from typing import Type

class MessageType(Enum):
    Note = 0
    CC = 1

class MessageFilterType(Enum):
    Channel = 0
    Note = 1
    Velocity = 2
    CC_Controller = 3
    CC_Value = 4

class InputRule:
    def __init__(self,
                 msg_type : Type[MessageType],
                 filters : dict[Type[MessageFilterType], int],
                 formula : str,
                 ):
        self.msg_type = msg_type
        self.filters = filters
        self.formula = formula

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

    sendMidi = pyqtSignal(list) # list of byte values

    def process_messages(self, messages):
        for msg in messages:
            self.sendMidi.emit(msg.bytes())
    
    @pyqtSlot(list)
    def receiveMidi(self, bytes):
        pass
    
    @pyqtSlot(int, int, int)
    def loop_state_changed(self, track, index, state):
        if (track, index) in self._loop_states_cache and self._loop_states_cache[(track, index)] == state:
            return # already sent this
        
        if state in self._loop_state_formulas:
            self.process_messages(eval_formula(self._loop_state_formulas[state], {**{'track': track, 'loop': index, 'state': state}, **self._substitution_rules}))
            self._loop_states_cache[(track, index)] = state
        elif self._loop_state_default_formula:
            self.process_messages(eval_formula(self._loop_state_default_formula, {**{'track': track, 'loop': index, 'state': state}, **self._substitution_rules}))
            self._loop_states_cache[(track, index)] = state

    @pyqtSlot(int)
    def active_sripting_section_changed(self, idx):
        if self._scripting_section_cache == idx:
            return # already sent this

        if self._scripting_section_formula:
            self.process_messages(eval_formula(self._scripting_section_formula, {'section': idx}))
            self._scripting_section_cache = idx
    
    @pyqtSlot(int)
    def active_scene_changed(self, idx):
        if self._active_scene_cache == idx:
            return # already sent this
        
        if self._active_scene_formula:
            self.process_messages(eval_formula(self._active_scene_formula, {'scene': idx}))
            self._active_scene_cache = idx