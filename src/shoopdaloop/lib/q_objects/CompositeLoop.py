from typing import *

from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer, QMetaObject, Qt
from PySide6.QtQuick import QQuickItem
from PySide6.QtQml import QJSValue

from .ShoopPyObject import *

from ..backend_wrappers import *
from ..mode_helpers import is_playing_mode, is_running_mode, is_recording_mode
from ..q_objects.Backend import Backend
from ..findFirstParent import findFirstParent
from ..findChildItems import findChildItems
from ..logging import Logger

# Wraps a back-end FX chain.
class CompositeLoop(ShoopQQuickItem):
    def __init__(self, parent=None):
        super(CompositeLoop, self).__init__(parent)
        self._schedule = {}
        self.logger = Logger('Frontend.CompositeLoop')
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

        self.scheduleChanged.connect(self.update_n_cycles, Qt.DirectConnection)
        self.syncLoopChanged.connect(self.update_sync_position, Qt.DirectConnection)
        self.syncLoopChanged.connect(self.update_sync_length, Qt.DirectConnection)
        self.nCyclesChanged.connect(self.update_length, Qt.DirectConnection)
        self.syncLengthChanged.connect(self.update_length, Qt.DirectConnection)
        self.iterationChanged.connect(self.update_position, Qt.DirectConnection)
        self.syncLengthChanged.connect(self.update_position, Qt.DirectConnection)
        self.modeChanged.connect(self.update_position, Qt.DirectConnection)
        self.syncPositionChanged.connect(self.update_position, Qt.DirectConnection)
    
    cycled = Signal()

    ######################
    ## PROPERTIES
    ######################

    # schedule
    # See CompositeLoop.qml for the format of this property.
    scheduleChanged = Signal('QVariant')
    @ShoopProperty('QVariant', notify=scheduleChanged)
    def schedule(self):
        return self._schedule
    @schedule.setter
    def schedule(self, val):
        if isinstance(val, QJSValue):
            val = val.toVariant()
        self.logger.trace(lambda: f'schedule -> {val}')
        self._schedule = val
        self.scheduleChanged.emit(self._schedule)
    
    # sync_loop
    syncLoopChanged = Signal('QVariant')
    @ShoopProperty('QVariant', notify=syncLoopChanged)
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
                val.positionChanged.connect(self.update_sync_position, Qt.DirectConnection)
                val.lengthChanged.connect(self.update_sync_length, Qt.DirectConnection)
                val.cycled.connect(self.handle_sync_loop_trigger, Qt.DirectConnection)
                self.update_sync_position()
                self.update_sync_length()
            self.syncLoopChanged.emit(self._sync_loop)
    
    # running_loops
    runningLoopsChanged = Signal('QVariant')
    @ShoopProperty('QVariant', notify=runningLoopsChanged)
    def running_loops(self):
        return list(self._running_loops)
    @running_loops.setter
    def running_loops(self, val):
        if isinstance(val, QJSValue):
            val = val.toVariant()
        self._running_loops = set(val)
        self.runningLoopsChanged.emit(self._running_loops)

    # iteration
    iterationChanged = Signal(int)
    @ShoopProperty(int, notify=iterationChanged)
    def iteration(self):
        return self._iteration
    @iteration.setter
    def iteration(self, val):
        if self._iteration != val:
            self.logger.debug(lambda: f'iteration -> {val}')
            self._iteration = val
            self.iterationChanged.emit(self._iteration)

    # mode
    modeChanged = Signal(int)
    @ShoopProperty(int, notify=modeChanged)
    def mode(self):
        return self._mode
    @mode.setter
    def mode(self, val):
        if val != self._mode:
            self.logger.debug(lambda: f'mode -> {val}')
            self._mode = val
            self.modeChanged.emit(self._mode)
    
    # next_mode
    nextModeChanged = Signal(int)
    @ShoopProperty(int, notify=nextModeChanged)
    def next_mode(self):
        return self._next_mode
    @next_mode.setter
    def next_mode(self, val):
        if val != self._next_mode:
            self.logger.debug(lambda: f'next_mode -> {val}')
            self._next_mode = val
            self.nextModeChanged.emit(self._next_mode)
    
    # next_transition_delay
    nextTransitionDelayChanged = Signal(int)
    @ShoopProperty(int, notify=nextTransitionDelayChanged)
    def next_transition_delay(self):
        return self._next_transition_delay
    @next_transition_delay.setter
    def next_transition_delay(self, val):
        if val != self._next_transition_delay:
            self.logger.debug(lambda: f'next_transition_delay -> {val}')
            self._next_transition_delay = val
            self.nextTransitionDelayChanged.emit(self._next_transition_delay)

    # n_cycles
    nCyclesChanged = Signal(int)
    ShoopSlot()
    def update_n_cycles(self):
        v = (max([int(k) for k in self._schedule.keys()]) if self._schedule else 0)
        if v != self._n_cycles:
            self.logger.debug(lambda: f'n_cycles -> {v}')
            self._n_cycles = v
            self.nCyclesChanged.emit(v)
    @ShoopProperty(int, notify=nCyclesChanged)
    def n_cycles(self):
        return self._n_cycles

    # length
    lengthChanged = Signal(int)
    ShoopSlot()
    def update_length(self):
        v = self.sync_length * self.n_cycles
        if v != self._length:
            self.logger.trace(lambda: f'length -> {v}')
            self._length = v
            self.lengthChanged.emit(v)
    @ShoopProperty(int, notify=lengthChanged)
    def length(self):
        return self._length

    # kind
    kindChanged = Signal(str)
    @ShoopProperty(str, notify=kindChanged)
    def kind(self):
        return self._kind
    @kind.setter
    def kind(self, val):
        if val != self._kind:
            self.logger.debug(lambda: f'kind -> {val}')
            self._kind = val
            self.kindChanged.emit(self._kind)
    
    # sync_position
    syncPositionChanged = Signal(int)
    ShoopSlot()
    def update_sync_position(self):
        v = 0
        if self.sync_loop:
            v = self.sync_loop.property('position')
        if v != self._sync_position:
            self.logger.trace(lambda: f'sync_position -> {v}')
            self._sync_position = v
            self.syncPositionChanged.emit(v)
    @ShoopProperty(int, notify=syncPositionChanged)
    def sync_position(self):
        return self._sync_position
    
    # sync_length
    syncLengthChanged = Signal(int)
    ShoopSlot()
    def update_sync_length(self):
        v = 0
        if self.sync_loop:
            v = self.sync_loop.property('length')
        if v != self._sync_length:
            self.logger.trace(lambda: f'sync_length -> {v}')
            self._sync_length = v
            self.syncLengthChanged.emit(v)
    @ShoopProperty(int, notify=syncLengthChanged)
    def sync_length(self):
        return self._sync_length

    # position
    positionChanged = Signal(int)
    ShoopSlot()
    def update_position(self):
        v = self.iteration * self.sync_length
        if is_running_mode(self.mode):
            v += self.sync_position
        if v != self._position:
            self.logger.trace(lambda: f'position -> {v}')
            self._position = v
            self.positionChanged.emit(v)
    @ShoopProperty(int, notify=positionChanged)
    def position(self):
        return self._position
    
    ########################
    ## SLOTS
    ########################

    @ShoopSlot(int, int, bool)
    def transition(self, mode, delay, wait_for_sync):
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
    
    @ShoopSlot()
    def handle_sync_loop_trigger(self):
        self.logger.debug(lambda: 'handle sync cycle')

        if self.next_transition_delay == 0:
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
                if self.kind == 'script':
                    self.logger.debug(lambda: 'ending script')
                    self.transition(LoopMode.Stopped.value, 0, True)
                elif is_recording_mode(self.mode):
                    self.logger.debug(lambda: 'cycle: recording to playing')
                    # Recording ends next cycle, transition to playing
                    self.transition(LoopMode.Playing.value, 0, True)
                else:
                    self.logger.debug(lambda: 'cycling')
                    # Will cycle around - trigger the actions for next cycle
                    self.do_triggers(0, self.mode)

            if cycled:
                self.cycled.emit()

    #########################
    ## METHODS
    #########################

    def cancel_all(self):
        self.logger.trace(lambda: 'cancel_all')
        for loop in self.running_loops:
            loop.transition(LoopMode.Stopped.value, 0, True)
        self.running_loops = []

    def handle_transition(self, mode):
        self.logger.debug(lambda: f'handle_transition({mode})')
        self.next_transition_delay = -1
        if mode != self.mode:
            self.mode = mode
            if not is_running_mode(mode):
                self.iteration = 0

    def do_triggers(self, iteration, mode):
        schedule = self._schedule
        sched_keys = [int(k) for k in schedule.keys()]
        self.logger.debug(lambda: f'do_triggers({iteration}, {mode})')
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
                    self.runningLoopsChanged.emit(self._running_loops)
    
