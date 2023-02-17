from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
import re
import time
import os
import tempfile
import json
from typing import *

import sys
sys.path.append('../..')

import lib.backend as backend
from lib.mode_helpers import is_playing_mode
from lib.q_objects.BackendLoopAudioChannel import BackendLoopAudioChannel
from lib.q_objects.BackendLoopMidiChannel import BackendLoopMidiChannel

# Wraps a back-end loop.
class BackendLoop(QObject):
    # Other signals
    cycled = pyqtSignal()
    passed_halfway = pyqtSignal()

    def __init__(self, backend_loop : Type[backend.BackendLoop], parent=None):
        super(BackendLoop, self).__init__(parent)
        self._length = 1.0
        self._position = 0.0
        self._mode = backend.LoopMode.Unknown.value
        self._next_mode = None
        self._next_transition_delay = None
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
    positionChanged = pyqtSignal(float)
    @pyqtProperty(float, notify=positionChanged)
    def position(self):
        return self._position
    @position.setter
    def position(self, p):
        if self._position != p:
            self._position = p
            self.positionChanged.emit(p)

    # next_mode: first upcoming mode change
    nextModeChanged = pyqtSignal('QVariant')
    @pyqtProperty('QVariant', notify=nextModeChanged)
    def next_mode(self):
        return self._next_mode
    @next_mode.setter
    def next_mode(self, p):
        if self._next_mode != p:
            self._next_mode = p
            self.nextModeChanged.emit(p)
    
    # next_mode: first upcoming mode change
    nextTransitionDelayChanged = pyqtSignal('QVariant')
    @pyqtProperty('QVariant', notify=nextTransitionDelayChanged)
    def next_transition_delay(self):
        return self._next_transition_delay
    @next_transition_delay.setter
    def next_transition_delay(self, p):
        if self._next_transition_delay != p:
            self._next_transition_delay = p
            self.nextTransitionDelayChanged.emit(p)
    
    # audio_channels: audio channel objects associated with this loop
    audioChannelsChanged = pyqtSignal('QVariant')
    @pyqtProperty('QVariant', notify=audioChannelsChanged)
    def audio_channels(self):
        return self.findChildren(BackendLoopAudioChannel)
    
    # audio_channels: audio channel objects associated with this loop
    midiChannelsChanged = pyqtSignal('QVariant')
    @pyqtProperty('QVariant', notify=midiChannelsChanged)
    def midi_channels(self):
        return self.findChildren(BackendLoopMidiChannel)
    
    ###########
    ## SLOTS
    ###########

    # Update mode from the back-end.
    @pyqtSlot()
    def update(self):
        prev_before_halway = (self.length > 0 and self.position < self.length/2)
        prev_position = self.position
        prev_mode = self.mode

        state = self._backend_loop.get_state()
        self.mode = state.mode
        self.length = state.length
        self.position = state.position
        self.next_mode = state.maybe_next_mode
        self.next_transition_delay = state.maybe_next_delay

        after_halfway = (self.length > 0 and self.position >= self.length/2)
        if (after_halfway and prev_before_halway):
            self.passed_halfway.emit()
        if (self.position < prev_position and is_playing_mode(prev_mode) and is_playing_mode(self.mode)):
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
    
    @pyqtSlot(result=BackendLoopAudioChannel)
    def add_audio_channel(self):
        r = BackendLoopAudioChannel(self._backend_loop.add_audio_channel(), self)
        self.audioChannelsChanged.emit(self.audio_channels)
        return r
    
    @pyqtSlot(result=BackendLoopMidiChannel)
    def add_midi_channel(self):
        r = BackendLoopMidiChannel(self._backend_loop.add_midi_channel(), self)
        self.midiChannelsChanged.emit(self.midi_channels)
        return r