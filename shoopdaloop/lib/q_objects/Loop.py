import re
import time
import os
import tempfile
import json
from typing import *
import sys

from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
from PyQt6.QtQuick import QQuickItem

from ..backend_wrappers import *
from ..mode_helpers import is_playing_mode
from ..q_objects.Backend import Backend
from ..findFirstParent import findFirstParent
from ..findChildItems import findChildItems

# Wraps a back-end loop.
class Loop(QQuickItem):
    # Other signals
    cycled = pyqtSignal()
    passed_halfway = pyqtSignal()

    def __init__(self, parent=None):
        super(Loop, self).__init__(parent)
        self._position = 0
        self._mode = LoopMode.Unknown.value
        self._next_mode = LoopMode.Unknown.value
        self._next_transition_delay = -1
        self._length = 0
        self._sync_source = None
        self._initialized = False
        self._backend = None
        self._backend_loop = None
        self._display_peak = 0.0
        self._display_midi_notes_active = 0
        self._display_midi_events_triggered = 0

        self.rescan_parents()
        if not self._backend:
            self.parentChanged.connect(self.rescan_parents)

    ######################
    # PROPERTIES
    ######################
    
    # backend
    backendChanged = pyqtSignal(Backend)
    @pyqtProperty(Backend, notify=backendChanged)
    def backend(self):
        return self._backend
    @backend.setter
    def backend(self, l):
        if l and l != self._backend:
            if self._backend or self._backend_loop:
                raise Exception('May not change backend of existing port')
            self._backend = l
            self.maybe_initialize()
    
    # initialized
    initializedChanged = pyqtSignal(bool)
    @pyqtProperty(bool, notify=initializedChanged)
    def initialized(self):
        return self._initialized

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
    nextModeChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=nextModeChanged)
    def next_mode(self):
        return self._next_mode
    
    # next_mode: first upcoming mode change
    nextTransitionDelayChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=nextTransitionDelayChanged)
    def next_transition_delay(self):
        return self._next_transition_delay
    
    # synchronization source loop
    syncSourceChanged = pyqtSignal('QVariant')
    @pyqtProperty('QVariant', notify=syncSourceChanged)
    def sync_source(self):
        return self._sync_source
    @sync_source.setter
    def sync_source(self, loop):
        def do_set():
            if loop != self._sync_source:
                self._sync_source = loop
                self._backend_loop.set_sync_source(loop._backend_loop)
                self.syncSourceChanged.emit(loop)
        if self.initialized:
            do_set()
        else:
            self.initializedChanged.connect(do_set)
    
    # display_peak : overall audio peak of the loop
    displayPeakChanged = pyqtSignal(float)
    @pyqtProperty(float, notify=displayPeakChanged)
    def display_peak(self):
        return self._display_peak

    # display_midi_notes_active : sum of notes active in midi channels
    displayMidiNotesActiveChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=displayMidiNotesActiveChanged)
    def display_midi_notes_active(self):
        return self._display_midi_notes_active
    
    # display_midi_events_triggered : sum of events now processed in channels
    displayMidiEventsTriggeredChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=displayMidiEventsTriggeredChanged)
    def display_midi_events_triggered(self):
        return self._display_midi_events_triggered
    
    ###########
    ## SLOTS
    ###########

    @pyqtSlot(result=list)
    def audio_channels(self):
        import lib.q_objects.LoopAudioChannel as LoopAudioChannel
        return findChildItems(self, lambda c: isinstance(c, LoopAudioChannel.LoopAudioChannel))
    
    @pyqtSlot(result=list)
    def midi_channels(self):
        import lib.q_objects.LoopMidiChannel as LoopMidiChannel
        return findChildItems(self, lambda c: isinstance(c, LoopMidiChannel.LoopMidiChannel))

    # Update mode from the back-end.
    @pyqtSlot()
    def update(self):
        if not self.initialized:
            return
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
        prev_display_peak = self._display_peak
        prev_display_midi_notes_active = self._display_midi_notes_active
        prev_display_midi_events_triggered = self._display_midi_events_triggered

        state = self._backend_loop.get_state()
        self._mode = state.mode
        self._length = state.length
        self._position = state.position
        self._next_mode = (state.maybe_next_mode if state.maybe_next_mode != None else state.mode)
        self._next_transition_delay = (state.maybe_next_delay if state.maybe_next_delay != None else -1)
        self._display_peak = (max([c.output_peak for c in self.audio_channels()]) if len(self.audio_channels()) > 0 else 0.0)
        self._display_midi_notes_active = (sum([c.n_notes_active for c in self.midi_channels()]) if len(self.midi_channels()) > 0 else 0)
        self._display_midi_events_triggered = (sum([c.n_events_triggered for c in self.midi_channels()]) if len(self.midi_channels()) > 0 else 0)

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
        if prev_display_peak != self._display_peak:
            self.displayPeakChanged.emit(self._display_peak)
        if prev_display_midi_notes_active != self._display_midi_notes_active:
            self.displayMidiNotesActiveChanged.emit(self._display_midi_notes_active)
        if prev_display_midi_events_triggered != self._display_midi_events_triggered:
            self.displayMidiEventsTriggeredChanged.emit(self._display_midi_events_triggered)

        after_halfway = (self.length > 0 and self.position >= self.length/2)
        if (after_halfway and prev_before_halway):
            self.passed_halfway.emit()
        if (self.position < prev_position and is_playing_mode(prev_mode) and is_playing_mode(self.mode)):
            self.cycled.emit()
    
    @pyqtSlot(int, int, bool)
    def transition(self, mode, delay, wait_for_sync):
        if self.initialized:
            self._backend_loop.transition(LoopMode(mode), delay, wait_for_sync)

    @pyqtSlot(int)
    def clear(self, length):
        if self.initialized:
            self._backend_loop.clear(length)
    
    @pyqtSlot(int, result=BackendLoopMidiChannel)
    def add_audio_channel(self, mode):
        if self.initialized:
            return self._backend_loop.add_audio_channel(ChannelMode(mode))
    
    @pyqtSlot(int, result=BackendLoopMidiChannel)
    def add_midi_channel(self, mode):
        if self.initialized:
            return self._backend_loop.add_midi_channel(ChannelMode(mode))
    
    @pyqtSlot(list)
    def load_audio_data(self, sound_channels):
        if sound_channels is not None:
            self.stop(0, False)
            self.set_position(0)
            if len(sound_channels) != len(self.audio_channels()):
                print ("Loaded {} channels but loop has {} channels. Assigning data to channels in round-robin fashion."
                    .format(len(sound_channels), len(self.audio_channels())))
            for idx in range(len(self.audio_channels())):
                self.audio_channels()[idx].load_data(sound_channels[idx % len(sound_channels)])
            self.set_length(len(sound_channels[0]))
    
    @pyqtSlot()
    def close(self):
        if self._backend_loop:
            self._backend_loop.destroy()
            self._backend_loop = None
            self._initialized = False
            self.initializedChanged.emit(False)
    
    @pyqtSlot()
    def rescan_parents(self):
        maybe_backend = findFirstParent(self, lambda p: p and isinstance(p, QQuickItem) and p.inherits('Backend') and self._backend == None)
        if maybe_backend:
            self.backend = maybe_backend
    
    def maybe_initialize(self):
        if self._backend and self._backend.initialized and not self._backend_loop:
            self._backend_loop = self._backend.get_backend_obj().create_loop()
            if self._backend_loop:
                self._initialized = True
                self.update()
                self.initializedChanged.emit(True)