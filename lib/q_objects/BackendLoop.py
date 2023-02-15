from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
import re
import time
import os
import tempfile
import json

import sys
sys.path.append('../..')

import lib.backend as backend

# Wraps a back-end loop.
class BackendLoop(QObject):

    # mode change notifications
    lengthChanged = pyqtSignal(float)
    posChanged = pyqtSignal(float)
    modeChanged = pyqtSignal(int)
    nextModeChanged = pyqtSignal(int)
    nextModeCountdownChanged = pyqtSignal(int)
    volumeChanged = pyqtSignal(float)
    outputPeakChanged = pyqtSignal(float)

    # Other signals
    cycled = pyqtSignal()
    passed_halfway = pyqtSignal()

    def __init__(self, backend_loop : Type[backend.BackendLoop], parent=None):
        super(BackendLoop, self).__init__(parent)
        self._length = 1.0
        self._pos = 0.0
        self._mode = backend.LoopMode.Unknown.value()
        self._next_mode = backend.LoopMode.Unknown.value()
        self._next_mode_countdown = -1
        self._volume = 1.0
        self._output_peak = 0.0
        self._backend_loop = backend_loop

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
    
    ###########
    ## SLOTS
    ###########

    # Update mode from the back-end.
    @pyqtSlot()
    def update(self):
        state = self._backend_loop.get_state()
        self.mode = state.mode
        self.length = state.length
        self.pos = state.pos
        # TODO output peak, volume

    @pyqtSlot(int, bool)
    def record(self, delay, wait_for_soft_sync):
        self._backend_loop.transition(backend.LoopMode.Recording, delay, wait_for_soft_sync)
    
    @pyqtSlot(int, bool)
    def play(self, delay, wait_for_soft_sync):
        self._backend_loop.transition(backend.LoopMode.Playing, delay, wait_for_soft_sync)
    
    @pyqtSlot(int, bool)
    def stop(self, delay, wait_for_soft_sync):
        self._backend_loop.transition(backend.LoopMode.Stopped, delay, wait_for_soft_sync)