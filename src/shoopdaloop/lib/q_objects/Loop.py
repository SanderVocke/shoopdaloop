import re
import time
import os
import tempfile
import json
from typing import *
import sys

from .ShoopPyObject import *

from PySide6.QtCore import Qt, QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from ..backend_wrappers import *
from ..mode_helpers import is_playing_mode
from ..q_objects.Backend import Backend
from ..findFirstParent import findFirstParent
from ..findChildItems import findChildItems
from .Logger import Logger

# Wraps a back-end loop.
class Loop(ShoopQQuickItem):
    # Other signals
    cycled = ShoopSignal()

    def __init__(self, parent=None):
        super(Loop, self).__init__(parent)
        self._position = self._new_position = 0
        self._mode = self._new_mode = LoopMode.Unknown.value
        self._next_mode = self._new_next_mode = LoopMode.Unknown.value
        self._next_transition_delay = self._new_next_transition_delay = -1
        self._length = self._new_length = 0
        self._sync_source = self._new_sync_source = None
        self._initialized = False
        self._backend = None
        self._backend_loop = None
        self._display_peaks = self._new_display_peaks = []
        self._display_midi_notes_active = self._new_display_midi_notes_active = 0
        self._display_midi_events_triggered = self._new_display_midi_events_triggered = 0
        self.logger = Logger(self)
        self.logger.name = "Frontend.Loop"
        self._n_updates_pending = 0

        self.initializedChanged.connect(lambda i: self.logger.debug("initialized -> {}".format(i)))

        def rescan_on_parent_changed():
            if not self._backend:
                self.rescan_parents()
        self.parentChanged.connect(lambda: rescan_on_parent_changed())
        self.rescan_parents()
        
        
        self.update.connect(self.updateOnGuiThread, Qt.QueuedConnection)

    update = ShoopSignal()

    ######################
    # PROPERTIES
    ######################
    
    # backend
    backendChanged = ShoopSignal(Backend)
    @ShoopProperty(Backend, notify=backendChanged)
    def backend(self):
        return self._backend
    @backend.setter
    def backend(self, l):
        if l and l != self._backend:
            if self._backend or self._backend_loop:
                self.logger.throw_error('May not change backend of existing loop')
            self._backend = l
            self.logger.trace(lambda: 'Set backend -> {}'.format(l))
            self.maybe_initialize()
    
    # initialized
    initializedChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=initializedChanged)
    def initialized(self):
        return self._initialized

    # mode
    modeChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=modeChanged)
    def mode(self):
        return self._mode
    # Indirect setter via back-end
    @ShoopSlot(int, thread_protected=False)
    def set_mode(self, mode):
        self.logger.debug(lambda: 'Set mode -> {}'.format(LoopMode(mode)))
        self._backend_loop.set_mode(mode)

    # length: loop length in samples
    lengthChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=lengthChanged)
    def length(self):
        return self._length
    # Indirect setter via back-end
    @ShoopSlot(int, thread_protected=False)
    def set_length(self, length):
        self.logger.debug(lambda: 'Set length -> {}'.format(length))
        self._backend_loop.set_length(length)

    # position: loop playback position in samples
    positionChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=positionChanged)
    def position(self):
        return self._position
    # Indirect setter via back-end
    @ShoopSlot(int)
    def set_position(self, position, thread_protected=False):
        self.logger.debug(lambda: 'Set position -> {}'.format(position))
        self._backend_loop.set_position(position)

    # next_mode: first upcoming mode change
    nextModeChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=nextModeChanged)
    def next_mode(self):
        return self._next_mode
    
    # next_mode: first upcoming mode change
    nextTransitionDelayChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=nextTransitionDelayChanged)
    def next_transition_delay(self):
        return self._next_transition_delay
    
    # synchronization source loop
    syncSourceChanged = ShoopSignal('QVariant')
    @ShoopProperty('QVariant', notify=syncSourceChanged)
    def sync_source(self):
        return self._sync_source
    @sync_source.setter
    def sync_source(self, loop):
        def do_set():
            if loop != self._sync_source:
                self.logger.debug(lambda: 'Set sync source -> {}'.format(loop))
                self._sync_source = loop
                self._backend_loop.set_sync_source(loop.get_backend_loop() if loop else None)
                self.syncSourceChanged.emit(loop)
        if self.initialized:
            do_set()
        else:
            self.logger.debug(lambda: 'Defer set sync source -> {}'.format(loop))
            self.initializedChanged.connect(do_set)

    # display_peaks : overall audio peaks of the loop
    displayPeaksChanged = ShoopSignal(list)
    @ShoopProperty(list, notify=displayPeaksChanged)
    def display_peaks(self):
        return self._display_peaks

    # display_midi_notes_active : sum of notes active in midi channels
    displayMidiNotesActiveChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=displayMidiNotesActiveChanged)
    def display_midi_notes_active(self):
        return self._display_midi_notes_active
    
    # display_midi_events_triggered : sum of events now processed in channels
    displayMidiEventsTriggeredChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=displayMidiEventsTriggeredChanged)
    def display_midi_events_triggered(self):
        return self._display_midi_events_triggered
    
    ###########
    ## SLOTS
    ###########
    
    # For debugging
    @ShoopSlot(str)
    def set_logging_instance_identifier(self, id):
        self.logger.instanceIdentifier = id

    def get_audio_channels_impl(self):
        from .LoopAudioChannel import LoopAudioChannel
        return findChildItems(self, lambda c: isinstance(c, LoopAudioChannel))
    
    def get_midi_channels_impl(self):
        from .LoopMidiChannel import LoopMidiChannel
        return findChildItems(self, lambda c: isinstance(c, LoopMidiChannel))


    @ShoopSlot(result=list)
    def get_audio_channels(self):
        return self.get_audio_channels_impl()
    
    @ShoopSlot(result=list)
    def get_midi_channels(self):
        return self.get_midi_channels_impl()

    # Update on back-end thread.
    @ShoopSlot(thread_protected = False)
    def updateOnOtherThread(self):
        self.logger.trace(lambda: f'update on GUI thread (# {self._n_updates_pending}, initialized {self._initialized})')
        if not self._initialized:
            return
        
        audio_chans = self.get_audio_channels_impl()
        midi_chans = self.get_midi_channels_impl()
        
        for channel in audio_chans:
            channel.updateOnOtherThread()
        for channel in midi_chans:
            channel.updateOnOtherThread()

        state = self._backend_loop.get_state()
        self._new_mode = state.mode
        self._new_length = state.length
        self._new_position = state.position
        self._new_next_mode = (state.maybe_next_mode if state.maybe_next_mode != None else state.mode)
        self._new_next_transition_delay = (state.maybe_next_delay if state.maybe_next_delay != None else -1)
        self._new_display_peaks = [c._output_peak for c in [a for a in audio_chans if a._mode in [ChannelMode.Direct.value, ChannelMode.Wet.value]]]
        self._new_display_midi_notes_active = (sum([c._n_notes_active for c in midi_chans]) if len(midi_chans) > 0 else 0)
        self._new_display_midi_events_triggered = (sum([c._n_events_triggered for c in midi_chans]) if len(midi_chans) > 0 else 0)
        
        self._n_updates_pending += 1

        if self:
            self.update.emit()
    
    # Update on GUI thread.
    @ShoopSlot()
    def updateOnGuiThread(self):
        self.logger.trace(lambda: f'update on GUI thread (# {self._n_updates_pending}, initialized {self._initialized})')
        if not self._initialized:
            return
        if self._n_updates_pending == 0:
            return
        
        audio_chans = self.get_audio_channels_impl()
        midi_chans = self.get_midi_channels_impl()
        
        for channel in audio_chans:
            channel.updateOnOtherThread()
        for channel in midi_chans:
            channel.updateOnOtherThread()

        prev_position = self._position
        prev_mode = self._mode
        prev_length = self._length
        prev_next_mode = self._next_mode
        prev_next_delay = self._next_transition_delay
        prev_display_peaks = self._display_peaks
        prev_display_midi_notes_active = self._display_midi_notes_active
        prev_display_midi_events_triggered = self._display_midi_events_triggered

        self._mode = self._new_mode
        self._length = self._new_length
        self._position = self._new_position
        self._next_mode = self._new_next_mode
        self._next_transition_delay = self._new_next_transition_delay
        self._display_peaks = self._new_display_peaks
        self._display_midi_notes_active = self._new_display_midi_notes_active
        self._display_midi_events_triggered = self._new_display_midi_events_triggered

        self._n_updates_pending = 0

        if prev_mode != self._mode:
            self.logger.debug(lambda: 'mode -> {}'.format(LoopMode(self._mode)))
            self.modeChanged.emit(self._mode)
        if prev_length != self._length:
            self.logger.trace(lambda: 'length -> {}'.format(self._length))
            self.lengthChanged.emit(self._length)
        if prev_position != self._position:
            self.logger.trace(lambda: 'pos -> {}'.format(self._position))
            self.positionChanged.emit(self._position)
        if prev_next_mode != self._next_mode:
            self.logger.debug(lambda: 'next mode -> {}'.format(LoopMode(self._next_mode)))
            self.nextModeChanged.emit(self._next_mode)
        if prev_next_delay != self._next_transition_delay:
            self.logger.debug(lambda: 'next transition -> {}'.format(self._next_transition_delay))
            self.nextTransitionDelayChanged.emit(self._next_transition_delay)
        if prev_display_peaks != self._display_peaks:
            self.displayPeaksChanged.emit(self._display_peaks)
        if prev_display_midi_notes_active != self._display_midi_notes_active:
            self.displayMidiNotesActiveChanged.emit(self._display_midi_notes_active)
        if prev_display_midi_events_triggered != self._display_midi_events_triggered:
            self.displayMidiEventsTriggeredChanged.emit(self._display_midi_events_triggered)

        if (self.position < prev_position and is_playing_mode(prev_mode) and is_playing_mode(self.mode)):
            self.logger.debug(lambda: 'cycled')
            self.cycled.emit()
    
    @ShoopSlot(int, int, bool)
    def transition(self, mode, delay, wait_for_sync):
        if self.initialized:
            if wait_for_sync and not self.sync_source:
                self.logger.warning(lambda: "Synchronous transition requested but no sync loop set")
            self._backend_loop.transition(LoopMode(mode), delay, wait_for_sync)
    
    @ShoopSlot(list, int, int, bool)
    def transition_multiple(self, loops, mode, delay, wait_for_sync):
        if self.initialized:
            backend_loops = [l._backend_loop for l in loops]
            BackendLoop.transition_multiple(backend_loops, LoopMode(mode), delay, wait_for_sync)

    @ShoopSlot(int)
    def clear(self, length):
        if self.initialized:
            self.logger.debug(lambda: 'clear')
            self._backend_loop.clear(length)
            for c in self.get_audio_channels():
                c.clear()
            for c in self.get_midi_channels():
                c.clear()
    
    @ShoopSlot(int, result=BackendLoopMidiChannel)
    def add_audio_channel(self, mode):
        if self.initialized:
            self.logger.debug(lambda: 'add audio channel')
            return self._backend_loop.add_audio_channel(ChannelMode(mode))
    
    @ShoopSlot(int, result=BackendLoopMidiChannel)
    def add_midi_channel(self, mode):
        if self.initialized:
            self.logger.debug(lambda: 'add midi channel')
            return self._backend_loop.add_midi_channel(ChannelMode(mode))
    
    @ShoopSlot(list)
    def load_audio_data(self, sound_channels):
        if sound_channels is not None:
            self.logger.info(lambda: 'Loading {} audio channels'.format(len(sound_channels)))
            self.stop(0, False)
            self.set_position(0)
            if len(sound_channels) != len(self.audio_channels()):
                self.logger.warning(lambda: "Loaded {} channels but loop has {} channels. Assigning data to channels in round-robin fashion."
                    .format(len(sound_channels), len(self.audio_channels())))
            for idx in range(len(self.audio_channels())):
                self.audio_channels()[idx].load_data(sound_channels[idx % len(sound_channels)])
            self.set_length(len(sound_channels[0]))
    
    @ShoopSlot()
    def close(self):
        if self._backend_loop:
            self.logger.debug(lambda: 'close')
            if self._backend:
                self._backend.unregisterBackendObject(self)
            self._backend_loop.destroy()
            self._backend_loop = None
            self._initialized = False
            self.initializedChanged.emit(False)
    
    @ShoopSlot()
    def rescan_parents(self):
        maybe_backend = findFirstParent(self, lambda p: p and isinstance(p, QQuickItem) and p.inherits('Backend') and self._backend == None)
        if maybe_backend:
            self.backend = maybe_backend
    
    @ShoopSlot(result='QVariant')
    def get_backend(self):
        maybe_backend = findFirstParent(self, lambda p: p and isinstance(p, QQuickItem) and p.inherits('Backend'))
        if maybe_backend:
            return maybe_backend
        self.logger.throw_error("Could not find backend!")
    
    @ShoopSlot(result="QVariant")
    def get_backend_loop(self):
        return self._backend_loop
    
    @ShoopSlot(result="QVariant")
    def py_loop(self):
        print(self)
        return self
    
    def maybe_initialize(self):
        if self._backend and self._backend.initialized and not self._backend_loop:
            self.logger.debug(lambda: 'Found backend, initializing')
            self._backend_loop = self._backend.get_backend_session_obj().create_loop()
            if self._backend_loop:
                self._initialized = True
                self.update()
                self._backend.registerBackendObject(self)
                self.initializedChanged.emit(True)
            else:
                self.logger.warning(lambda: 'Failed to create loop')