from PyQt6.QtCore import QObject, pyqtSignal, pyqtSlot

from ..LoopState import LoopState
from ..MidiScripting import *

# Base class for loopers to be used by ShoopDaLoop.
class ControlOutputManager(QObject):
    def __init__(self, parent=None):
        super(ControlOutputManager, self).__init__(parent)
        self._loop_state_formulas = {} # Loop state value to formula
        self._loop_state_default_formula = None # Used if there is no formula for a given state
        self._scripting_section_formula = None
        self._active_scene_formula = None

        self._loop_states_cache = {} # tuple of (track, loop) => last transmitted state
        self._scripting_section_cache = None # last transmitted section
        self._active_scene_cache = None # last transmitted active scene

    sendMidi = pyqtSignal(list) # list of byte values

    def process_messages(self, messages):
        for msg in messages:
            self.sendMidi.emit(msg.bytes())
    
    @pyqtSlot(int, int, int)
    def loop_state_changed(self, track, index, state):
        if (track, index) in self._loop_states_cache and self._loop_states_cache[(track, index)] == state:
            return # already sent this
        
        if state in self._loop_state_formulas:
            self.process_messages(eval_formula(self._loop_state_formulas[state], {'track': track, 'loop': index, 'state': state}))
            self._loop_states_cache[(track, index)] = state
        elif self._loop_state_default_formula:
            self.process_messages(eval_formula(self._loop_state_default_formula, {'track': track, 'loop': index, 'state': state}))
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