import re
import time
import os
import tempfile
import json

from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer

from ..StatesAndActions import LoopMode, LoopActionType

# Represents the mode of any looper.
class LooperState(QObject):

    # mode change notifications
    lengthChanged = pyqtSignal(float)
    posChanged = pyqtSignal(float)
    modeChanged = pyqtSignal(int)
    nextModeChanged = pyqtSignal(int)
    nextModeCountdownChanged = pyqtSignal(int)
    volumeChanged = pyqtSignal(float)
    outputPeakChanged = pyqtSignal(float)
    selectedChanged = pyqtSignal(bool)
    targetedChanged = pyqtSignal(bool)

    # Other signals
    cycled = pyqtSignal()
    passed_halfway = pyqtSignal()

    def __init__(self, parent=None):
        super(LooperState, self).__init__(parent)
        self._length = 1.0
        self._pos = 0.0
        self._mode = LoopMode.Unknown.value
        self._next_mode = LoopMode.Unknown.value
        self._next_mode_countdown = -1
        self._volume = 1.0
        self._output_peak = 0.0
        self._get_waveforms_fn = lambda from_sample, to_sample, samples_per_bin: {}
        self._get_midi_fn = lambda: []
        self._selected = False
        self._targeted = False

    ######################
    # PROPERTIES
    ######################

    # mode
    modeChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=modeChanged)
    def mode(self):
        return self._mode
    @mode.setter
    def mode(self, s):
        if self._mode != s:
            self._mode = s
            self.modeChanged.emit(s)
    
    # next mode
    nextModeChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=nextModeChanged)
    def next_mode(self):
        return self._next_mode
    @next_mode.setter
    def next_mode(self, s):
        if self._next_mode != s:
            self._next_mode = s
            self.nextModeChanged.emit(s)
    
    # next mode countdown. Decrements on every trigger
    # until at 0, the next mode is transitioned to
    nextModeCountdownChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=nextModeCountdownChanged)
    def next_mode_countdown(self):
        return self._next_mode_countdown
    @next_mode_countdown.setter
    def next_mode_countdown(self, s):
        if self._next_mode_countdown != s:
            self._next_mode_countdown = s
            self.nextModeCountdownChanged.emit(s)

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

    # position: loop playback position in seconds
    posChanged = pyqtSignal(float)
    @pyqtProperty(float, notify=posChanged)
    def position(self):
        return self._pos
    @position.setter
    def position(self, p):
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

    # selected: whether the loop is in the current selection.
    # this is not something known to the back-end.
    selectedChanged = pyqtSignal(bool)
    @pyqtProperty(bool, notify=selectedChanged)
    def selected(self):
        return self._selected
    @selected.setter
    def selected(self, p):
        if self._selected != p:
            self._selected = p
            self.selectedChanged.emit(p)
    
    # targeted: whether the loop is the current "target".
    # this is not something known to the back-end.
    targetedChanged = pyqtSignal(bool)
    @pyqtProperty(bool, notify=targetedChanged)
    def targeted(self):
        return self._targeted
    @targeted.setter
    def targeted(self, p):
        if self._targeted != p:
            self._targeted = p
            self.targetedChanged.emit(p)

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
    
    def set_get_waveforms_fn(self, fn):
        self._get_waveforms_fn = fn
    
    def set_get_midi_fn(self, fn):
        self._get_midi_fn = fn
    
    @pyqtSlot(int, int, int, result='QVariant')
    def get_waveforms(self, from_sample, to_sample, samples_per_bin):
        return self._get_waveforms_fn (from_sample, to_sample, samples_per_bin)
    
    @pyqtSlot(result=list)
    def get_midi(self):
        return self._get_midi_fn()