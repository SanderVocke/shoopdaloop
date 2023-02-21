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
from lib.sound_file_io import load_audio_file

# Wraps a back-end loop.
class Loop(QObject):
    # Other signals
    cycled = pyqtSignal()
    passed_halfway = pyqtSignal()

    def __init__(self, parent=None):
        super(Loop, self).__init__(parent)
        self._backend_loop = backend.create_loop()
        self._position = 0
        self._mode = backend.LoopMode.Unknown.value
        self._next_mode = None
        self._next_transition_delay = None
        self._length = 0
        self.update()

    ######################
    # PROPERTIES
    ######################

    # mode
    modeChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=modeChanged)
    def mode(self):
        return self._mode
    # Indirect setter via back-end
    @pyqtSlot(int)
    def set_mode(self, mode):
        self._backend_loop.set_mode(mode)

    # length: loop length in samples
    lengthChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=lengthChanged)
    def length(self):
        return self._length
    # Indirect setter via back-end
    @pyqtSlot(int)
    def set_length(self, length):
        self._backend_loop.set_length(length)

    # position: loop playback position in samples
    positionChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=positionChanged)
    def position(self):
        return self._position
    # Indirect setter via back-end
    @pyqtSlot(int)
    def set_position(self, position):
        self._backend_loop.set_position(position)

    # next_mode: first upcoming mode change
    nextModeChanged = pyqtSignal('QVariant')
    @pyqtProperty('QVariant', notify=nextModeChanged)
    def next_mode(self):
        return self._next_mode
    
    # next_mode: first upcoming mode change
    nextTransitionDelayChanged = pyqtSignal('QVariant')
    @pyqtProperty('QVariant', notify=nextTransitionDelayChanged)
    def next_transition_delay(self):
        return self._next_transition_delay
    
    ###########
    ## SLOTS
    ###########

    @pyqtSlot(result=list)
    def audio_channels(self):
        import lib.q_objects.LoopAudioChannel as LoopAudioChannel
        return self.findChildren(LoopAudioChannel.LoopAudioChannel)
    
    @pyqtSlot(result=list)
    def midi_channels(self):
        import lib.q_objects.LoopMidiChannel as LoopMidiChannel
        return self.findChildren(LoopMidiChannel.LoopMidiChannel)

    # Update mode from the back-end.
    @pyqtSlot()
    def update(self):
        for channel in self.audio_channels():
            channel.update()
        for channel in self.midi_channels():
            channel.update()

        prev_before_halway = (self._length > 0 and self._position < self._length/2)
        prev_position = self._position
        prev_mode = self._mode
        prev_length = self._length
        prev_next_mode = self._next_mode
        prev_next_delay = self._next_transition_delay

        state = self._backend_loop.get_state()
        self._mode = state.mode
        self._length = state.length
        self._position = state.position
        self._next_mode = state.maybe_next_mode
        self._next_transition_delay = state.maybe_next_delay

        if prev_mode != self._mode:
            self.modeChanged.emit(self._mode)
        if prev_length != self._length:
            self.lengthChanged.emit(self._length)
        if prev_position != self._position:
            self.positionChanged.emit(self._position)
        if prev_next_mode != self._next_mode:
            self.nextModeChanged.emit(self._next_mode)
        if prev_next_delay != self._next_transition_delay:
            self.nextTransitionDelayChanged.emit(self._next_transition_delay)

        after_halfway = (self.length > 0 and self.position >= self.length/2)
        if (after_halfway and prev_before_halway):
            self.passed_halfway.emit()
        if (self.position < prev_position and is_playing_mode(prev_mode) and is_playing_mode(self.mode)):
            self.cycled.emit()

    @pyqtSlot(int, bool)
    def record(self, delay, wait_for_soft_sync):
        self._backend_loop.transition(backend.LoopMode.Recording, delay, wait_for_soft_sync)
    
    @pyqtSlot(int, bool)
    def play(self, delay, wait_for_soft_sync):
        self._backend_loop.transition(backend.LoopMode.Playing, delay, wait_for_soft_sync)
    
    @pyqtSlot(int, bool)
    def stop(self, delay, wait_for_soft_sync):
        self._backend_loop.transition(backend.LoopMode.Stopped, delay, wait_for_soft_sync)
    
    @pyqtSlot(int)
    def clear(self, length):
        self._backend_loop.clear(length)
    
    @pyqtSlot(result=backend.BackendLoopMidiChannel)
    def add_audio_channel(self, enabled):
        return self._backend_loop.add_audio_channel(enabled)
    
    @pyqtSlot(result=backend.BackendLoopMidiChannel)
    def add_midi_channel(self, enabled):
        return self._backend_loop.add_midi_channel(enabled)
    
    @pyqtSlot(list)
    def load_audio_data(self, sound_channels):
        if sound_channels is not None:
            self.stop(0, False)
            self.set_position(0)
            if len(sound_channels) != len(self.audio_channels):
                print ("Loaded {} channels but loop has {} channels. Assigning data to channels in round-robin fashion."
                    .format(len(sound_channels), len(self.audio_channels)))
            for idx in range(len(self.audio_channels)):
                self.audio_channels[idx].load_data(sound_channels[idx % len(sound_channels)])
            self.set_length(len(sound_channels[0]))
    
    @pyqtSlot(str, bool, int)
    def load_audio_file(self, filename, force_length, forced_length):
        sound_channels = load_audio_file(filename,
                                         backend.get_sample_rate(),
                                        (forced_length if force_length else None))
        self.load_audio_data(sound_channels)