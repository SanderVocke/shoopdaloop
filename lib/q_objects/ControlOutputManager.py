from PyQt6.QtCore import QObject, pyqtSignal, pyqtSlot

from ..LoopState import LoopState
from ..MidiScripting import parse_control_command_formula

# Base class for loopers to be used by ShoopDaLoop.
class ControlOutputManager(QObject):
    def __init__(self):
        self._loop_state_formulas = {} # Loop state value to formula
        self._loop_state_default_formula = None # Used if there is no formula for a given state

    sendMidi = pyqtSignal(list) # list of byte values
    
    @pyqtSlot(int, int, int)
    def loop_state_changed(track, index, state):
        if state in self._loop_state_formulas:
            self.process_messages(parse_control_command_formula(self._loop_state_formulas[state], {'track': track, 'index': index}))
        elif self._loop_state_default_formula:
            self.process_messages(parse_control_command_formula(self._loop_state_default_formula, {'track': track, 'index': index}))