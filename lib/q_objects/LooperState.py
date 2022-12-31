from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
import re
import time
import os
import tempfile
import json

from ..StatesAndActions import LoopState, LoopActionType

# Represents the state of any looper.
class LooperState(QObject):

    # State change notifications
    lengthChanged = pyqtSignal(float)
    posChanged = pyqtSignal(float)
    stateChanged = pyqtSignal(int)
    nextStateChanged = pyqtSignal(int)
    nextStateCountdownChanged = pyqtSignal(int)
    volumeChanged = pyqtSignal(float)
    outputPeakChanged = pyqtSignal(float)

    def __init__(self, parent=None):
        super(LooperState, self).__init__(parent)
        self._length = 1.0
        self._pos = 0.0
        self._state = LoopState.Unknown.value
        self._next_state = LoopState.Unknown.value
        self._next_state_countdown = -1
        self._volume = 1.0
        self._output_peak = 0.0

    ######################
    # PROPERTIES
    ######################

    # state
    stateChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=stateChanged)
    def state(self):
        return self._state
    @state.setter
    def state(self, s):
        if self._state != s:
            self._state = s
            self.stateChanged.emit(s)
    
    # next state
    nextStateChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=nextStateChanged)
    def next_state(self):
        return self._next_state
    @next_state.setter
    def next_state(self, s):
        if self._next_state != s:
            self._next_state = s
            self.nextStateChanged.emit(s)
    
    # next state countdown. Decrements on every trigger
    # until at 0, the next state is transitioned to
    nextStateCountdownChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=nextStateCountdownChanged)
    def next_state_countdown(self):
        return self._next_state_countdown
    @next_state_countdown.setter
    def next_state_countdown(self, s):
        if self._next_state_countdown != s:
            self._next_state_countdown = s
            self.nextStateCountdownChanged.emit(s)

    # length: loop length in seconds
    lengthChanged = pyqtSignal(float)
    @pyqtProperty(float, notify=lengthChanged)
    def length(self):
        return self._length
    @length.setter
    def length(self, l):
        if self._length != l:
            self._length = l
            self.lengthChanged.emit(l)

    # pos: loop playback position in seconds
    posChanged = pyqtSignal(float)
    @pyqtProperty(float, notify=posChanged)
    def pos(self):
        return self._pos
    @pos.setter
    def pos(self, p):
        if self._pos != p:
            self._pos = p
            self.posChanged.emit(p)
    
    # Volume: volume of playback in 0.0-1.0
    @pyqtProperty(float, notify=volumeChanged)
    def volume(self):
        return self._volume
    
    @volume.setter
    def volume(self, p):
        if self._volume != p:
            self._volume = p
            self.volumeChanged.emit(p)
    
    # Output peak: amplitude value of 0.0-...
    @pyqtProperty(float, notify=outputPeakChanged)
    def outputPeak(self):
        return self._output_peak
    
    @outputPeak.setter
    def outputPeak(self, p):
        if self._output_peak != p:
            self._output_peak = p
            self.outputPeakChanged.emit(p)

    @pyqtSlot(result=str)
    def serialize_session_state(self):
        d = {
            'volume' : self.volume,
            'length' : self.length,
        }
        return json.dumps(d)
    
    @pyqtSlot(str)
    def deserialize_session_state(self, data):
        d = json.loads(data)
        self.volume = d['volume']
        self.length = d['length']