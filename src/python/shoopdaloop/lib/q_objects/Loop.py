import re
import time
import os
import tempfile
import json
from typing import *
import sys

from .ShoopPyObject import *
from .FindParentBackend import FindParentBackend

from PySide6.QtCore import Qt, QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from ..backend_wrappers import *
from ..mode_helpers import is_playing_mode
from ..q_objects.Backend import Backend
from ..findFirstParent import findFirstParent
from ..findChildItems import findChildItems
from .Logger import Logger

import traceback

# Wraps a back-end loop.
class Loop(FindParentBackend):
    # Other signals
    cycled = ShoopSignal(int)
    cycledUnsafe = ShoopSignal(int, thread_protection=ThreadProtectionType.OtherThread)

    def __init__(self, parent=None):
        super(Loop, self).__init__(parent)
        self._position = 0
        self._mode = LoopMode.Unknown.value
        self._next_mode = LoopMode.Unknown.value
        self._next_transition_delay = -1
        self._length = self._new_length = 0
        self._sync_source = None
        self._initialized = False
        self._backend_loop = None
        self._display_peaks = []
        self._display_midi_notes_active = 0
        self._display_midi_events_triggered = 0
        self.logger = Logger(self)
        self.logger.name = "Frontend.Loop"
        self._pending_transitions = []
        self._pending_cycles = 0
        self._cycle_nr = 0

        self.modeChangedUnsafe.connect(self.modeChanged, Qt.QueuedConnection)
        self.nextModeChangedUnsafe.connect(self.nextModeChanged, Qt.QueuedConnection)
        self.lengthChangedUnsafe.connect(self.lengthChanged, Qt.QueuedConnection)
        self.positionChangedUnsafe.connect(self.positionChanged, Qt.QueuedConnection)
        self.nextTransitionDelayChangedUnsafe.connect(self.nextTransitionDelayChanged, Qt.QueuedConnection)
        self.displayPeaksChangedUnsafe.connect(self.displayPeaksChanged, Qt.QueuedConnection)
        self.displayMidiNotesActiveChangedUnsafe.connect(self.displayMidiNotesActiveChanged, Qt.QueuedConnection)
        self.displayMidiEventsTriggeredChangedUnsafe.connect(self.displayMidiEventsTriggeredChanged, Qt.QueuedConnection)

        self.cycledUnsafe.connect(self.cycled, Qt.QueuedConnection)

        self._signal_sender = ThreadUnsafeSignalEmitter()
        self._signal_sender.signal.connect(self.updateOnGuiThread, Qt.QueuedConnection)

        self.backendChanged.connect(lambda: self.maybe_initialize())
        self.backendInitializedChanged.connect(lambda: self.maybe_initialize())

        self.initializedChanged.connect(lambda i: self.logger.debug("initialized -> {}".format(i)))

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
    modeChangedUnsafe = ShoopSignal(int, thread_protection=ThreadProtectionType.AnyThread) # Signal will be triggered on any thread
    modeChanged = ShoopSignal(int) # on GUI thread only, for e.g. bindings
    @ShoopProperty(int, notify=modeChanged, thread_protection=ThreadProtectionType.AnyThread)
    def mode(self):
        return self._mode
    # Indirect setter via back-end
    @ShoopSlot(int, thread_protection=ThreadProtectionType.AnyThread)
    def set_mode(self, mode):
        self.logger.debug(lambda: 'Set mode -> {}'.format(LoopMode(mode)))
        self._backend_loop.set_mode(mode)

    # length: loop length in samples
    lengthChangedUnsafe = ShoopSignal(int, thread_protection=ThreadProtectionType.AnyThread) # Signal will be triggered on any thread
    lengthChanged = ShoopSignal(int) # on GUI thread only, for e.g. bindings
    @ShoopProperty(int, notify=lengthChanged, thread_protection=ThreadProtectionType.AnyThread)
    def length(self):
        return self._length
    # Indirect setter via back-end
    @ShoopSlot(int, thread_protection=ThreadProtectionType.AnyThread)
    def set_length(self, length):
        self.logger.debug(lambda: 'Set length -> {}'.format(length))
        self._backend_loop.set_length(length)

    # position: loop playback position in samples
    positionChangedUnsafe = ShoopSignal(int, thread_protection=ThreadProtectionType.AnyThread) # Signal will be triggered on any thread
    positionChanged = ShoopSignal(int) # on GUI thread only, for e.g. bindings
    @ShoopProperty(int, notify=positionChanged, thread_protection=ThreadProtectionType.AnyThread)
    def position(self):
        return self._position
    # Indirect setter via back-end
    @ShoopSlot(int, thread_protection=ThreadProtectionType.AnyThread)
    def set_position(self, position):
        self.logger.debug(lambda: 'Set position -> {}'.format(position))
        self._backend_loop.set_position(position)

    # next_mode: first upcoming mode change
    nextModeChangedUnsafe = ShoopSignal(int, thread_protection=ThreadProtectionType.AnyThread) # Signal will be triggered on any thread
    nextModeChanged = ShoopSignal(int) # on GUI thread only, for e.g. bindings
    @ShoopProperty(int, notify=nextModeChanged, thread_protection=ThreadProtectionType.AnyThread)
    def next_mode(self):
        return self._next_mode
    
    # next_mode: first upcoming mode change
    nextTransitionDelayChangedUnsafe = ShoopSignal(int, thread_protection=ThreadProtectionType.AnyThread) # Signal will be triggered on any thread
    nextTransitionDelayChanged = ShoopSignal(int) # on GUI thread only, for e.g. bindings
    @ShoopProperty(int, notify=nextTransitionDelayChanged, thread_protection=ThreadProtectionType.AnyThread)
    def next_transition_delay(self):
        return self._next_transition_delay
    
    # synchronization source loop
    syncSourceChanged = ShoopSignal('QVariant')
    @ShoopProperty('QVariant', notify=syncSourceChanged, thread_protection=ThreadProtectionType.AnyThread)
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
    displayPeaksChangedUnsafe = ShoopSignal(list, thread_protection=ThreadProtectionType.AnyThread) # Signal will be triggered on any thread
    displayPeaksChanged = ShoopSignal(list) # on GUI thread only, for e.g. bindings
    @ShoopProperty(list, notify=displayPeaksChanged, thread_protection=ThreadProtectionType.AnyThread)
    def display_peaks(self):
        return self._display_peaks

    # display_midi_notes_active : sum of notes active in midi channels
    displayMidiNotesActiveChangedUnsafe = ShoopSignal(int, thread_protection=ThreadProtectionType.AnyThread) # Signal will be triggered on any thread
    displayMidiNotesActiveChanged = ShoopSignal(int) # on GUI thread only, for e.g. bindings
    @ShoopProperty(int, notify=displayMidiNotesActiveChanged, thread_protection=ThreadProtectionType.AnyThread)
    def display_midi_notes_active(self):
        return self._display_midi_notes_active
    
    # display_midi_events_triggered : sum of events now processed in channels
    displayMidiEventsTriggeredChangedUnsafe = ShoopSignal(int, thread_protection=ThreadProtectionType.AnyThread) # Signal will be triggered on any thread
    displayMidiEventsTriggeredChanged = ShoopSignal(int) # on GUI thread only, for e.g. bindings
    @ShoopProperty(int, notify=displayMidiEventsTriggeredChanged, thread_protection=ThreadProtectionType.AnyThread)
    def display_midi_events_triggered(self):
        return self._display_midi_events_triggered
    
    # instanceIdentifier (for clarity in debugging)
    instanceIdentifierChanged = ShoopSignal(str)
    @ShoopProperty(str, notify=instanceIdentifierChanged, thread_protection=ThreadProtectionType.AnyThread)
    def instanceIdentifier(self):
        return self.logger.instanceIdentifier
    @instanceIdentifier.setter
    def instanceIdentifier(self, l):
        if l and l != self.logger.instanceIdentifier:
            self.logger.instanceIdentifier = l
            self.instanceIdentifierChanged.emit(l)
    
    ###########
    ## SLOTS
    ###########

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
    @ShoopSlot(thread_protection = ThreadProtectionType.OtherThread)
    def updateOnOtherThread(self):
        #self.logger.trace(lambda: f'update on GUI thread (# {self._n_updates_pending}, initialized {self._initialized})')
        if not self._initialized:
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

        state = self._backend_loop.get_state()
        self._mode = state.mode
        self._length = state.length
        self._position = state.position
        self._next_mode = (state.maybe_next_mode if state.maybe_next_mode != None else state.mode)
        self._next_transition_delay = (state.maybe_next_delay if state.maybe_next_delay != None else -1)
        self._display_peaks = [c._output_peak for c in [a for a in audio_chans if a._mode in [ChannelMode.Direct.value, ChannelMode.Wet.value]]]
        self._display_midi_notes_active = (sum([c._n_notes_active for c in midi_chans]) if len(midi_chans) > 0 else 0)
        self._display_midi_events_triggered = (sum([c._n_events_triggered for c in midi_chans]) if len(midi_chans) > 0 else 0)
        
        if prev_mode != self._mode:
            self.logger.debug(lambda: 'mode -> {}'.format(LoopMode(self._mode)))
            self.modeChangedUnsafe.emit(self._mode)
        if prev_length != self._length:
            self.logger.trace(lambda: 'length -> {}'.format(self._length))
            self.lengthChangedUnsafe.emit(self._length)
        if prev_position != self._position:
            self.logger.trace(lambda: 'pos -> {}'.format(self._position))
            self.positionChangedUnsafe.emit(self._position)
        if prev_next_mode != self._next_mode:
            self.logger.debug(lambda: 'next mode -> {}'.format(LoopMode(self._next_mode)))
            self.nextModeChangedUnsafe.emit(self._next_mode)
        if prev_next_delay != self._next_transition_delay:
            self.logger.debug(lambda: 'next transition -> {}'.format(self._next_transition_delay))
            self.nextTransitionDelayChangedUnsafe.emit(self._next_transition_delay)
        if prev_display_peaks != self._display_peaks:
            self.displayPeaksChangedUnsafe.emit(self._display_peaks)
        if prev_display_midi_notes_active != self._display_midi_notes_active:
            self.displayMidiNotesActiveChangedUnsafe.emit(self._display_midi_notes_active)
        if prev_display_midi_events_triggered != self._display_midi_events_triggered:
            self.displayMidiEventsTriggeredChangedUnsafe.emit(self._display_midi_events_triggered)

        for transition in self._pending_transitions:
            if isinstance(transition[0], list):
                self.transition_multiple_impl(*transition)
            else:
                self.transition_impl(*transition)
        self._pending_transitions = []

        if (self.position < prev_position and is_playing_mode(prev_mode) and is_playing_mode(self.mode)):
            self._cycle_nr += 1
            self.logger.debug(lambda: f'cycled -> nr {self._cycle_nr}')
            self.cycledUnsafe.emit(self._cycle_nr)
    
    # Update on GUI thread.
    @ShoopSlot()
    def updateOnGuiThread(self):
        #self.logger.trace(lambda: f'update on GUI thread (# {self._n_updates_pending}, initialized {self._initialized})')
        if not self._initialized or not self.isValid():
            return
        
        audio_chans = self.get_audio_channels_impl()
        midi_chans = self.get_midi_channels_impl()
        
        for channel in audio_chans:
            channel.updateOnGuiThread()
        for channel in midi_chans:
            channel.updateOnGuiThread()
    
    @ShoopSlot(int, int, int, thread_protection=ThreadProtectionType.AnyThread)
    def transition(self, mode, maybe_delay, maybe_align_to_sync_at):
        self._pending_transitions.append([mode, maybe_delay, maybe_align_to_sync_at])

    def transition_impl(self, mode, maybe_delay, maybe_align_to_sync_at):
        if self._initialized:
            if maybe_delay >= 0 and not self._sync_source:
                self.logger.warning(lambda: "Synchronous transition requested but no sync loop set")
            self._backend_loop.transition(LoopMode(mode), maybe_delay, maybe_align_to_sync_at)
    
    @ShoopSlot(list, int, int, int)
    def transition_multiple(self, loops, mode, maybe_delay, maybe_align_to_sync_at):
        self._pending_transitions.append([loops, mode, maybe_delay, maybe_align_to_sync_at])

    def transition_multiple_impl(self, loops, mode, maybe_delay, maybe_align_to_sync_at):
        if self._initialized:
            backend_loops = [l._backend_loop for l in loops]
            BackendLoop.transition_multiple(backend_loops, LoopMode(mode), maybe_delay, maybe_align_to_sync_at)
    
    @ShoopSlot('QVariant', 'QVariant', 'QVariant', int)
    def adopt_ringbuffers(self, reverse_start_cycle, cycles_length, go_to_cycle, go_to_mode):
        if self._initialized:
            self.logger.debug(lambda: f'adopt ringbuffer contents @ {reverse_start_cycle}, len {cycles_length}, go to cycle {go_to_cycle}, go to mode {go_to_mode}')
            self._backend_loop.adopt_ringbuffer_contents(reverse_start_cycle, cycles_length, go_to_cycle, go_to_mode)

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
        return self

    # Another loop which references this loop (composite) can notify this loop that it is
    # about to handle a sync loop cycle in advance, to ensure a deterministic ordering.
    @ShoopSlot(int, thread_protection = ThreadProtectionType.OtherThread)
    def dependent_will_handle_sync_loop_cycle(self, cycle_nr):
        pass
    
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