from typing import *

from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer, QMetaObject, Qt
from PySide6.QtQuick import QQuickItem
from PySide6.QtQml import QJSValue

from .FindParentBackend import FindParentBackend
from .ShoopPyObject import *

from ..backend_wrappers import *
from ..mode_helpers import is_playing_mode, is_running_mode, is_recording_mode
from ..q_objects.Backend import Backend
from ..q_objects.Logger import Logger
from ..findFirstParent import findFirstParent
from ..findChildItems import findChildItems
from ..recursive_jsvalue_convert import recursively_convert_jsvalue

from collections.abc import Mapping, Sequence

import traceback

# Manage a back-end composite loop, keeps running if GUI thread stalls
class CompositeLoop(FindParentBackend):
    def __init__(self, parent=None):
        super(CompositeLoop, self).__init__(parent)
        self._schedule = {}
        self.logger = Logger(self)
        self.logger.name = 'Frontend.CompositeLoop'
        self._sync_loop = None
        self._running_loops = set()
        self._iteration = 0
        self._mode = LoopMode.Stopped.value
        self._next_mode = LoopMode.Stopped.value
        self._next_transition_delay = -1
        self._length = 0
        self._position = 0
        self._sync_position = 0
        self._sync_length = 0
        self._n_cycles = 0
        self._kind = 'regular'
        self._pending_transitions = []
        self._pending_cycles = 0
        self._backend = None
        self._initialized = False

        self.scheduleChangedUnsafe.connect(self.scheduleChanged, Qt.QueuedConnection)
        self.nCyclesChangedUnsafe.connect(self.nCyclesChanged, Qt.QueuedConnection)
        self.syncLengthChangedUnsafe.connect(self.syncLengthChanged, Qt.QueuedConnection)
        self.iterationChangedUnsafe.connect(self.iterationChanged, Qt.QueuedConnection)
        self.modeChangedUnsafe.connect(self.modeChanged, Qt.QueuedConnection)
        self.syncPositionChangedUnsafe.connect(self.syncPositionChanged, Qt.QueuedConnection)
        self.nextModeChangedUnsafe.connect(self.nextModeChanged, Qt.QueuedConnection)
        self.nextTransitionDelayChangedUnsafe.connect(self.nextTransitionDelayChanged, Qt.QueuedConnection)
        self.runningLoopsChangedUnsafe.connect(self.runningLoopsChanged, Qt.QueuedConnection)
        self.kindChangedUnsafe.connect(self.kindChanged, Qt.QueuedConnection)
        self.lengthChangedUnsafe.connect(self.lengthChanged, Qt.QueuedConnection)
        self.positionChangedUnsafe.connect(self.positionChanged, Qt.QueuedConnection)
        
        self.syncLoopChanged.connect(self.update_sync_position, Qt.QueuedConnection)
        self.syncLoopChanged.connect(self.update_sync_length, Qt.QueuedConnection)

        self.cycledUnsafe.connect(self.cycled, Qt.QueuedConnection)
        self.iterationChangedUnsafe.connect(self.update_position, Qt.DirectConnection)
        self.modeChangedUnsafe.connect(self.update_position, Qt.DirectConnection)

        self.backendChanged.connect(lambda: self.maybe_initialize())
        self.backendInitializedChanged.connect(lambda: self.maybe_initialize())
    
    cycled = ShoopSignal()
    cycledUnsafe = ShoopSignal()

    ######################
    ## PROPERTIES
    ######################

    # schedule
    # See CompositeLoop.qml for the format of this property.
    scheduleChangedUnsafe = ShoopSignal('QVariant', thread_protection=ThreadProtectionType.AnyThread) # Signal will be triggered on any thread
    scheduleChanged = ShoopSignal('QVariant') # on GUI thread only, for e.g. bindings
    @ShoopProperty('QVariant', notify=scheduleChanged, thread_protection=ThreadProtectionType.AnyThread)
    def schedule(self):
        return self._schedule
    @schedule.setter
    def schedule(self, val):
        # The schedule may arrive as a JSValue and loops in the schedule may be JSValues too
        val = recursively_convert_jsvalue(val)
        self.logger.trace(lambda: f'schedule -> {val}')
        self._schedule = val
        self.scheduleChangedUnsafe.emit(self._schedule)
        n = (max([int(k) for k in self._schedule.keys()]) if self._schedule else 0)
        if n != self._n_cycles:
            self.logger.debug(lambda: f'n_cycles -> {n}')
            self._n_cycles = n
            self.nCyclesChangedUnsafe.emit(n)
            self.update_length()

    # initialized
    initializedChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=initializedChanged)
    def initialized(self):
        return self._initialized
    
    # sync_loop
    syncLoopChanged = ShoopSignal('QVariant')
    @ShoopProperty('QVariant', notify=syncLoopChanged, thread_protection=ThreadProtectionType.AnyThread)
    def sync_loop(self):
        return self._sync_loop
    @sync_loop.setter
    def sync_loop(self, val):
        if val != self._sync_loop:
            if self._iteration != val:
                self.logger.debug(lambda: f'sync_loop -> {val}')
            if self._sync_loop:
                self._sync_loop.disconnect(self)
            self._sync_loop = val
            if val:
                if hasattr(val, 'positionChangedUnsafe'):
                    self.logger.debug(lambda: 'connecting to sync loop (back-end thread)')
                    val.positionChangedUnsafe.connect(self.update_sync_position_with_value, Qt.DirectConnection)
                    val.lengthChangedUnsafe.connect(self.update_sync_length_with_value, Qt.DirectConnection)
                    val.cycledUnsafe.connect(self.handle_sync_loop_trigger, Qt.DirectConnection)
                else:
                    self.logger.warning(lambda: 'connecting to sync loop on GUI thread. Sync loop: {val}')
                    val.positionChanged.connect(self.update_sync_position, Qt.AutoConnection)
                    val.lengthChanged.connect(self.update_sync_length, Qt.AutoConnection)
                    val.cycled.connect(self.handle_sync_loop_trigger, Qt.AutoConnection)
                self.update_sync_position()
                self.update_sync_length()
            self.syncLoopChanged.emit(self._sync_loop)
    
    # running_loops
    runningLoopsChangedUnsafe = ShoopSignal('QVariant', thread_protection=ThreadProtectionType.AnyThread) # Signal will be triggered on any thread
    runningLoopsChanged = ShoopSignal('QVariant') # on GUI thread only, for e.g. bindings
    @ShoopProperty('QVariant', notify=runningLoopsChanged, thread_protection=ThreadProtectionType.AnyThread)
    def running_loops(self):
        return list(self._running_loops)
    @running_loops.setter
    def running_loops(self, val):
        if isinstance(val, QJSValue):
            val = val.toVariant()
        self._running_loops = set(val)
        self.runningLoopsChangedUnsafe.emit(self._running_loops)

    # iteration
    iterationChangedUnsafe = ShoopSignal(int, thread_protection=ThreadProtectionType.AnyThread) # Signal will be triggered on any thread
    iterationChanged = ShoopSignal(int) # on GUI thread only, for e.g. bindings
    @ShoopProperty(int, notify=iterationChanged, thread_protection=ThreadProtectionType.AnyThread)
    def iteration(self):
        return self._iteration
    @iteration.setter
    def iteration(self, val):
        if self._iteration != val:
            self.logger.debug(lambda: f'iteration -> {val}')
            self._iteration = val
            self.iterationChangedUnsafe.emit(self._iteration)

    # mode
    modeChangedUnsafe = ShoopSignal(int, thread_protection=ThreadProtectionType.AnyThread) # Signal will be triggered on any thread
    modeChanged = ShoopSignal(int) # on GUI thread only, for e.g. bindings
    @ShoopProperty(int, notify=modeChanged, thread_protection=ThreadProtectionType.AnyThread)
    def mode(self):
        return self._mode
    @mode.setter
    def mode(self, val):
        if val != self._mode:
            self.logger.debug(lambda: f'mode -> {val}')
            self._mode = val
            self.modeChangedUnsafe.emit(self._mode)
    
    # next_mode
    nextModeChangedUnsafe = ShoopSignal(int, thread_protection=ThreadProtectionType.AnyThread) # Signal will be triggered on any thread
    nextModeChanged = ShoopSignal(int) # on GUI thread only, for e.g. bindings
    @ShoopProperty(int, notify=nextModeChanged, thread_protection=ThreadProtectionType.AnyThread)
    def next_mode(self):
        return self._next_mode
    @next_mode.setter
    def next_mode(self, val):
        if val != self._next_mode:
            self.logger.debug(lambda: f'next_mode -> {val}')
            self._next_mode = val
            self.nextModeChangedUnsafe.emit(self._next_mode)
    
    # next_transition_delay
    nextTransitionDelayChangedUnsafe = ShoopSignal(int, thread_protection=ThreadProtectionType.AnyThread) # Signal will be triggered on any thread
    nextTransitionDelayChanged = ShoopSignal(int) # on GUI thread only, for e.g. bindings
    @ShoopProperty(int, notify=nextTransitionDelayChanged, thread_protection=ThreadProtectionType.AnyThread)
    def next_transition_delay(self):
        return self._next_transition_delay
    @next_transition_delay.setter
    def next_transition_delay(self, val):
        if val != self._next_transition_delay:
            self.logger.debug(lambda: f'next_transition_delay -> {val}')
            self._next_transition_delay = val
            self.nextTransitionDelayChangedUnsafe.emit(self._next_transition_delay)

    # n_cycles
    nCyclesChangedUnsafe = ShoopSignal(int, thread_protection=ThreadProtectionType.AnyThread) # Signal will be triggered on any thread
    nCyclesChanged = ShoopSignal(int) # on GUI thread only, for e.g. bindings
    @ShoopProperty(int, notify=nCyclesChanged, thread_protection=ThreadProtectionType.AnyThread)
    def n_cycles(self):
        return self._n_cycles

    # length
    lengthChangedUnsafe = ShoopSignal(int, thread_protection=ThreadProtectionType.AnyThread) # Signal will be triggered on any thread
    lengthChanged = ShoopSignal(int) # on GUI thread only, for e.g. bindings
    @ShoopSlot(thread_protection=ThreadProtectionType.AnyThread)
    def update_length(self, *args):
        self.logger.trace(lambda: 'updating length')
        v = self._sync_length * self._n_cycles
        if v != self._length:
            self.logger.trace(lambda: f'length -> {v}')
            self._length = v
            self.lengthChangedUnsafe.emit(v)
    @ShoopProperty(int, notify=lengthChanged, thread_protection=ThreadProtectionType.AnyThread)
    def length(self):
        return self._length

    # kind
    kindChangedUnsafe = ShoopSignal(str, thread_protection=ThreadProtectionType.AnyThread) # Signal will be triggered on any thread
    kindChanged = ShoopSignal(str) # on GUI thread only, for e.g. bindings
    @ShoopProperty(str, notify=kindChanged, thread_protection=ThreadProtectionType.AnyThread)
    def kind(self):
        return self._kind
    @kind.setter
    def kind(self, val):
        if val != self._kind:
            self.logger.debug(lambda: f'kind -> {val}')
            self._kind = val
            self.kindChangedUnsafe.emit(self._kind)
    
    # sync_position
    syncPositionChangedUnsafe = ShoopSignal(int, thread_protection=ThreadProtectionType.AnyThread) # Signal will be triggered on any thread
    syncPositionChanged = ShoopSignal(int) # on GUI thread only, for e.g. bindings
    
    @ShoopSlot(int, thread_protection=ThreadProtectionType.AnyThread)
    def update_sync_position_with_value(self, v):
        self.logger.trace(lambda: 'updating sync position with value from signal')
        if v != self._sync_position:
            self.logger.trace(lambda: f'sync_position -> {v}')
            self._sync_position = v
            self.syncPositionChangedUnsafe.emit(v)
            self.update_position()
    
    @ShoopSlot()
    def update_sync_position(self, *args):
        self.logger.trace(lambda: 'updating sync position synchronously')
        v = 0
        if self.sync_loop:
            v = self.sync_loop.property('position')
        if v != self._sync_position:
            self.logger.trace(lambda: f'sync_position -> {v}')
            self._sync_position = v
            self.syncPositionChangedUnsafe.emit(v)
            self.update_position()
    @ShoopProperty(int, notify=syncPositionChanged, thread_protection=ThreadProtectionType.AnyThread)
    def sync_position(self):
        return self._sync_position
    
    # sync_length
    syncLengthChangedUnsafe = ShoopSignal(int, thread_protection=ThreadProtectionType.AnyThread) # Signal will be triggered on any thread
    syncLengthChanged = ShoopSignal(int) # on GUI thread only, for e.g. bindings
    
    @ShoopSlot(int, thread_protection=ThreadProtectionType.AnyThread)
    def update_sync_length_with_value(self, v):
        self.logger.trace(lambda: 'updating sync length with value from signal')
        if v != self._sync_length:
            self.logger.trace(lambda: f'sync_length -> {v}')
            self._sync_length = v
            self.syncLengthChangedUnsafe.emit(v)
            self.update_length()
            self.update_position()
    
    @ShoopSlot()
    def update_sync_length(self, *args):
        self.logger.trace(lambda: 'updating sync length synchronously')
        v = 0
        if self.sync_loop:
            v = self.sync_loop.property('length')
        if v != self._sync_length:
            self.logger.trace(lambda: f'sync_length -> {v}')
            self._sync_length = v
            self.syncLengthChangedUnsafe.emit(v)
            self.update_length()
            self.update_position()
    @ShoopProperty(int, notify=syncLengthChanged, thread_protection=ThreadProtectionType.AnyThread)
    def sync_length(self):
        return self._sync_length

    # position
    positionChangedUnsafe = ShoopSignal(int, thread_protection=ThreadProtectionType.AnyThread) # Signal will be triggered on any thread
    positionChanged = ShoopSignal(int) # on GUI thread only, for e.g. bindings
    @ShoopSlot(thread_protection=ThreadProtectionType.AnyThread)
    def update_position(self, *args):
        self.logger.trace(lambda: 'updating position')
        v = max(0, self._iteration) * self._sync_length
        if is_running_mode(self.mode):
            v += self._sync_position
        if v != self._position:
            self.logger.trace(lambda: f'position -> {v}')
            self._position = v
            self.positionChangedUnsafe.emit(v)
    @ShoopProperty(int, notify=positionChanged, thread_protection=ThreadProtectionType.AnyThread)
    def position(self):
        return self._position
    
    @ShoopSlot(int, int, bool, thread_protection=ThreadProtectionType.AnyThread)
    def transition(self, mode, delay, wait_for_sync):
        self._pending_transitions.append([mode, delay, wait_for_sync])

    def transition_impl(self, mode, delay, wait_for_sync):
        self.logger.debug(lambda: f'transition -> {mode} @ {delay} (wait {wait_for_sync})')
        if not (self.n_cycles > 0) and is_recording_mode(self.mode):
            # We cannot record a composite loop if the lengths of the other loops are not yet set.
            return

        if not is_running_mode(self.mode) and is_running_mode(mode):
            self.iteration = -1
        
        self.next_transition_delay = delay
        self.next_mode = mode
        if self.next_transition_delay == 0:
            if is_running_mode(mode):
                self.do_triggers(0, mode)
            else:
                self.cancel_all()

        if not wait_for_sync:
            self.handle_sync_loop_trigger()
    
    @ShoopSlot(thread_protection=ThreadProtectionType.AnyThread)
    def handle_sync_loop_trigger(self):
        self._pending_cycles += 1

    def handle_sync_loop_trigger_impl(self):
        self.logger.debug(lambda: 'handle sync cycle')

        if self._next_transition_delay == 0:
            self.handle_transition(self.next_mode)
        elif self.next_transition_delay > 0:
            self.next_transition_delay = self.next_transition_delay - 1
            if self.next_transition_delay == 0:
                self.do_triggers(0, self.next_mode)

        if is_running_mode(self.mode):
            cycled = False
            self.iteration = self.iteration + 1
            if (self.iteration >= self.n_cycles):
                self.iteration = 0
                cycled = True
            
            self.do_triggers(self.iteration+1, self.mode)
            if ((self.iteration+1) >= self.n_cycles):
                self.logger.debug(lambda: 'preparing cycle end')
                if self.kind == 'script' and not (self.next_transition_delay >= 0 and is_running_mode(self.next_mode)):
                    self.logger.debug(lambda: f'ending script {self.mode} {self.next_mode} {self.next_transition_delay}')
                    self.transition_impl(LoopMode.Stopped.value, 0, True)
                elif is_recording_mode(self.mode):
                    self.logger.debug(lambda: 'cycle: recording to playing')
                    # Recording ends next cycle, transition to playing
                    self.transition_impl(LoopMode.Playing.value, 0, True)
                else:
                    self.logger.debug(lambda: 'cycling')
                    # Will cycle around - trigger the actions for next cycle
                    self.do_triggers(0, self.mode)

            if cycled:
                self.cycledUnsafe.emit()
                
        self.logger.trace(lambda: 'handle sync cycle done')
    
    def cancel_all(self):
        self.logger.trace(lambda: 'cancel_all')
        for loop in self._running_loops:
            loop.transition(LoopMode.Stopped.value, 0, True)
        self.running_loops = []

    def handle_transition(self, mode):
        self.logger.debug(lambda: f'handle_transition({mode})')
        self.next_transition_delay = -1
        if mode != self._mode:
            self.mode = mode
            if not is_running_mode(mode):
                self.iteration = 0

    def do_triggers(self, iteration, mode):
        schedule = self._schedule
        sched_keys = [int(k) for k in schedule.keys()]
        self.logger.debug(lambda: f'{self.kind} do_triggers({iteration}, {mode})')
        if iteration in sched_keys:
            elem = schedule[str(iteration)]
            loops_end = elem['loops_end']
            loops_start = elem['loops_start']
            for loop in loops_end:
                self.logger.debug(lambda: f'loop end: {loop}')
                loop.transition(LoopMode.Stopped.value, 0, True)
                if loop in self._running_loops:
                    self._running_loops.remove(loop)
                self.runningLoopsChanged.emit(self._running_loops)
            if is_running_mode(mode):
                for entry in loops_start:
                    loop = entry[0]
                    loop_mode = entry[1]
                    
                    if loop_mode == None:
                        # Automatic mode based on our own mode
                        loop_mode = mode
                        # Special case is if we are recording. In that case, the same loop may be scheduled to
                        # record multiple times. The desired behavior is to just record it once and then stop it.
                        # That allows the artist to keep playing to fill the gap if monitoring, or to just stop
                        # playing if not monitoring, and in both cases the resulting recording is the first iteration.
                        handled = False
                        if is_recording_mode(mode):
                            # To implement the above: see if we have already recorded.
                            for i in range(iteration):
                                if i in sched_keys:
                                    other_starts = schedule[str(i)]['loops_start']
                                    if isinstance(other_starts, QJSValue):
                                        other_starts = other_starts.toVariant()
                                    if loop in other_starts:
                                        # We have already recorded this loop. Don't record it again.
                                        self.logger.debug(lambda: f'Not re-recording {loop}')
                                        loop.transition(LoopMode.Stopped.value, 0, True)
                                        self._running_loops.remove(loop)
                                        self.runningLoopsChanged.emit(self._running_loops)
                                        handled = True
                                        break

                        if handled:
                            continue

                    self.logger.debug(lambda: f'loop start: {loop}')
                    loop.transition(loop_mode, 0, True)
                    self._running_loops.add(loop)
                    self.runningLoopsChangedUnsafe.emit(self._running_loops)
    
    def maybe_initialize(self):
        if self._backend and self._backend.initialized and not self._initialized:
            self.logger.debug(lambda: 'Found backend, initializing')
            self.initializedChanged.emit(True)
    
    # Update from the back-end.
    @ShoopSlot(thread_protection = ThreadProtectionType.OtherThread)
    def updateOnOtherThread(self):
        if self._backend:
            for _ in range(self._pending_cycles):
                self.handle_sync_loop_trigger_impl()
            for transition in self._pending_transitions:
                self.transition_impl(*transition)
            self._pending_transitions = []
            self._pending_cycles = 0
    
