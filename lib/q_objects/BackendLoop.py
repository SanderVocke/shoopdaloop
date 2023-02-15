from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
import re
import time
import os
import tempfile
import json

import sys
sys.path.append('../..')

import lib.backend as backend
from lib.mode_helpers import is_playing_mode

# Wraps a back-end loop.
class BackendLoop(QObject):
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
    
    ###########
    ## SLOTS
    ###########

    # Update mode from the back-end.
    @pyqtSlot()
    def update(self):
        prev_before_halway = (self.length > 0 and self.pos < self.length/2)
        prev_pos = self.pos
        prev_mode = self.mode

        state = self._backend_loop.get_state()
        self.mode = state.mode
        self.length = state.length
        self.pos = state.pos

        after_halfway = (self.length > 0 and self.pos >= self.length/2)
        if (after_halfway and prev_before_halway):
            self.passed_halfway.emit()
        if (self.pos < prev_pos and is_playing_mode(prev_mode) and is_playing_mode(self.mode)):
            self.cycled.emit()

        # TODO output peak, volume, other channel-related stuff

    @pyqtSlot(int, bool)
    def record(self, delay, wait_for_soft_sync):
        self._backend_loop.transition(backend.LoopMode.Recording, delay, wait_for_soft_sync)
    
    @pyqtSlot(int, bool)
    def play(self, delay, wait_for_soft_sync):
        self._backend_loop.transition(backend.LoopMode.Playing, delay, wait_for_soft_sync)
    
    @pyqtSlot(int, bool)
    def stop(self, delay, wait_for_soft_sync):
        self._backend_loop.transition(backend.LoopMode.Stopped, delay, wait_for_soft_sync)