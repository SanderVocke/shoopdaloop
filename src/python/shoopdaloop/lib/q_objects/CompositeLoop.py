from typing import *

from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer, QMetaObject, Qt
from PySide6.QtQuick import QQuickItem
from PySide6.QtQml import QJSValue

from .FindParentBackend import FindParentBackend
from .ShoopPyObject import *

from ..backend_wrappers import *
from ..mode_helpers import is_playing_mode, is_running_mode, is_recording_mode
from ..findFirstParent import findFirstParent
from ..findChildItems import findChildItems
from ..recursive_jsvalue_convert import recursively_convert_jsvalue
from ..q_objects.Backend import Backend
from ..q_objects.Logger import Logger

from collections.abc import Mapping, Sequence

import traceback
import math

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
        self._pending_cycles = []
        self._backend = None
        self._initialized = False
        self._play_after_record = False
        self._sync_mode_active = False
        self._cycle_nr = 0
        self._last_handled_source_cycle_nr = -1

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
        self.playAfterRecordChangedUnsafe.connect(self.playAfterRecordChanged, Qt.QueuedConnection)
        
        self.syncLoopChanged.connect(self.update_sync_position, Qt.QueuedConnection)
        self.syncLoopChanged.connect(self.update_sync_length, Qt.QueuedConnection)

        self.cycledUnsafe.connect(self.cycled, Qt.QueuedConnection)
        self.iterationChangedUnsafe.connect(self.update_position, Qt.DirectConnection)
        self.modeChangedUnsafe.connect(self.update_position, Qt.DirectConnection)

        self.backendChanged.connect(lambda: self.maybe_initialize())
        self.backendInitializedChanged.connect(lambda: self.maybe_initialize())
    
    cycled = ShoopSignal(int)
    cycledUnsafe = ShoopSignal(int)

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
        def stringify_schedule (schedule):
            rval = ''
            for iteration,elem in schedule.items():
                rval += f'{iteration}:'
                for to_stop in elem['loops_end']:
                    rval += f'\n- {to_stop.instanceIdentifier} -> Stopped'
                for to_start in elem['loops_start']:
                    rval += f'\n- {to_start[0].instanceIdentifier} -> {to_start[1] if to_start[1] != None else "Autostart"}'
                for to_ignore in elem['loops_ignored']:
                    rval += f'\n- {to_ignore.instanceIdentifier} ignored'
                rval += "\n"
            return rval

        self.logger.trace(lambda: f'schedule updated:\n{stringify_schedule(val)}')
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

    # play_after_record
    playAfterRecordChangedUnsafe = ShoopSignal(bool, thread_protection=ThreadProtectionType.AnyThread) # Signal will be triggered on any thread
    playAfterRecordChanged = ShoopSignal(bool) # on GUI thread only, for e.g. bindings
    @ShoopProperty(int, notify=playAfterRecordChanged, thread_protection=ThreadProtectionType.AnyThread)
    def play_after_record(self):
        return self._play_after_record
    @play_after_record.setter
    def play_after_record(self, val):
        if self._play_after_record != val:
            self.logger.debug(lambda: f'play after record -> {val}')
            self._play_after_record = val
            self.playAfterRecordChangedUnsafe.emit(self._play_after_record)
    
    # sync_mode_active
    syncModeActiveChangedUnsafe = ShoopSignal(bool, thread_protection=ThreadProtectionType.AnyThread) # Signal will be triggered on any thread
    syncModeActiveChanged = ShoopSignal(bool) # on GUI thread only, for e.g. bindings
    @ShoopProperty(int, notify=syncModeActiveChanged, thread_protection=ThreadProtectionType.AnyThread)
    def sync_mode_active(self):
        return self._sync_mode_active
    @sync_mode_active.setter
    def sync_mode_active(self, val):
        if self._sync_mode_active != val:
            self.logger.debug(lambda: f'sync mode active -> {val}')
            self._sync_mode_active = val
            self.syncModeActiveChangedUnsafe.emit(self._sync_mode_active)

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
    
    @ShoopSlot(int, int, int, thread_protection=ThreadProtectionType.AnyThread)
    def transition(self, mode, maybe_delay, maybe_to_sync_at_cycle):
        self.logger.trace(lambda: f'queue transition -> {mode} : wait {maybe_delay}, align @ {maybe_to_sync_at_cycle}')
        self._pending_transitions.append([mode, maybe_delay, maybe_to_sync_at_cycle])

    def transition_impl(self, mode, maybe_delay, maybe_to_sync_at_cycle):
        self.logger.debug(lambda: f'transition -> {mode} : wait {maybe_delay}, align @ {maybe_to_sync_at_cycle}')
        if maybe_to_sync_at_cycle >= 0:
            self.transition_with_immediate_sync_impl(mode, maybe_to_sync_at_cycle)
        else:
            self.transition_default_impl(mode, maybe_delay)

    def list_transitions(self, mode, start_cycle, end_cycle):
        # Proceed through the schedule up to the point we want to go by calling our trigger function with a callback just
        # recording the transitions.
        transitions = {}
        schedule = self._schedule
        sched_keys = [int(i) for i in schedule.keys()]
        for i in range(start_cycle, sched_keys[-1] + 1):
            transitions[i] = []
            self.do_triggers(i, mode, lambda self, loop, mode: transitions[i].append({'loop': loop, 'mode': mode}))
            if i >= end_cycle:
                break
        return transitions
       
    def transition_default_impl(self, mode, maybe_delay):
        if not is_running_mode(self.mode) and is_running_mode(mode):
            self.iteration = -1
        
        self.next_transition_delay = max(maybe_delay, 0)
        self.next_mode = mode
        if self.next_transition_delay == 0:
            if is_running_mode(mode):
                self.do_triggers(0, mode)
            else:
                self.cancel_all()

        if maybe_delay < 0:
            self.handle_sync_loop_trigger()

    def transition_with_immediate_sync_impl(self, mode, sync_cycle):
        # Proceed through the schedule up to the point we want to go by calling our trigger function with a callback just
        # recording the transitions.
        transitions = self.list_transitions(mode, 0, sync_cycle)
        
        if self.logger.should_trace():
            str = 'immediate sync transition - virtual transition list:'
            for iteration,ts in transitions.items():
                for t in ts:
                    str = str + f'\n - iteration {iteration}: {t["loop"].instanceIdentifier} -> {t["mode"]}'
            self.logger.trace(lambda: str)
        
        # Find the last transition for each loop up until this point.
        last_loop_transitions = {}
        for it,ts in transitions.items():
            for t in ts:
                last_loop_transitions[t['loop']] = {'mode': t['mode'], 'iteration': it}
        
        # Get all the currently active loops into their correct mode and cycle.
        for loop,elem in last_loop_transitions.items():
            n_cycles_ago = sync_cycle - elem['iteration']
            n_cycles = 1
            if loop.sync_source:
                n_cycles = math.ceil(loop.length / loop.sync_source.length)
            mode = elem['mode']
            
            current_cycle = None
            if t['mode'] in [LoopMode.Playing.value, LoopMode.Replacing.value, LoopMode.PlayingDryThroughWet.value, LoopMode.RecordingDryIntoWet]:
                # loop around
                current_cycle = n_cycles_ago % max(n_cycles, 1)
            elif t['mode'] == LoopMode.Recording.value:
                # keep going indefinitely
                current_cycle = n_cycles_ago
            
            self.logger.trace(lambda: f'loop {loop.instanceIdentifier} -> {mode}, goto cycle {current_cycle} ({elem["mode"]} triggered {n_cycles_ago} cycles ago, loop length {n_cycles} cycles)')
            loop.transition(mode, DontWaitForSync,
                (current_cycle if mode != LoopMode.Stopped.value else DontAlignToSyncImmediately)
            )
        
        # Apply our own mode change
        self.mode = mode
        self.iteration = sync_cycle
        self.update_length()
        self.update_position()
        self.logger.trace(lambda: f'immediate sync: Done - mode -> {mode}, iteration -> {sync_cycle}, position -> {self._position}')

        # Perform the trigger(s) for the next loop cycle
        self.do_triggers(self.iteration + 1, mode)
    
    @ShoopSlot(int, thread_protection=ThreadProtectionType.AnyThread)
    def handle_sync_loop_trigger(self, cycle_nr):
        self.logger.trace(lambda: f'queue sync loop trigger')
        self._pending_cycles.append(cycle_nr)

    @ShoopSlot('QVariant', 'QVariant', 'QVariant', int)
    def adopt_ringbuffers(self, reverse_start_cycle, cycles_length, go_to_cycle, go_to_mode):
        if not self._sync_loop or self._sync_length <= 0:
            self.logger.warning(lambda: f'ignoring grab - undefined or empty sync loop')
            return
        self.logger.trace(lambda: f'adopt ringbuffer and go to cycle {go_to_cycle}, go to mode {go_to_mode}')

        # Proceed through the schedule up to the point we want to go by calling our trigger function with a callback just
        # recording the transitions.
        transitions = self.list_transitions(LoopMode.Recording.value, 0, self._n_cycles)

        if self.logger.should_trace():
            str = 'ringbuffer grab - virtual transition list:'
            for iteration,ts in transitions.items():
                for t in ts:
                    str = str + f'\n - iteration {iteration}: {t["loop"].instanceIdentifier} -> {t["mode"]}'
            self.logger.trace(lambda: str)
        
        # Find the first recording range for each loop.
        loop_recording_starts = {}
        loop_recording_ends = {}
        for it,ts in transitions.items():
            for t in ts:
                if t['mode'] == LoopMode.Recording.value:
                    # Store only the first recording start.
                    loop_recording_starts[t['loop']] = min(
                        it,
                        loop_recording_starts[t['loop']]
                    ) if t['loop'] in loop_recording_starts.keys() else it
        for it,ts in transitions.items():
            for t in ts:
                if t['mode'] != LoopMode.Recording.value and t['loop'] in loop_recording_starts.keys() and it > loop_recording_starts[t['loop']]:
                    loop_recording_ends[t['loop']] = min(
                        it,
                        loop_recording_ends[t['loop']]
                    ) if t['loop'] in loop_recording_ends.keys() else it
        
        to_grab = []
        for loop,start_it in loop_recording_starts.items():
            end_it = (loop_recording_ends[loop] if loop in loop_recording_ends.keys() else self._n_cycles)
            grab_n = max(end_it - start_it, 1)
            reverse_start_offset = self._n_cycles - start_it
            if not self._sync_mode_active:
                # If sync mode is inactive, we want to end up inside the last cycle.
                reverse_start_offset = max(0, reverse_start_offset - 1)
            to_grab.append({'loop': loop, 'reverse_start': reverse_start_offset, 'n': grab_n})
            self.logger.trace(f"to grab: {loop.instanceIdentifier} @ reverse start {reverse_start_offset}, n = {grab_n}")

        for item in to_grab:
            item['loop'].adopt_ringbuffers(item['reverse_start'], item['n'], 0, LoopMode.Unknown.value)
        
        if go_to_mode != LoopMode.Unknown.value:
            self.transition(go_to_mode, DontWaitForSync, go_to_cycle)

    def all_loops(self):
        loops = set()
        for elem in self._schedule.values():
            for e in elem['loops_start']:
                loops.add(e[0])
            for e in elem['loops_end']:
                loops.add(e)
            for e in elem['loops_ignored']:
                loops.add(e)
        return loops

    def handle_sync_loop_trigger_impl(self, cycle_nr):
        if cycle_nr == self._last_handled_source_cycle_nr:
            self.logger.trace(lambda: f'already handled sync cycle {cycle_nr}, skipping')
            return
        self.logger.debug(lambda: f'handle sync cycle {cycle_nr}')

        # Before we start, give any of the loops in our schedule a chance to handle the cycle
        # first. This ensures a deterministic ordering of execution.
        for l in self.all_loops():
            l.dependent_will_handle_sync_loop_cycle(cycle_nr)

        if self._next_transition_delay == 0:
            self.handle_transition(self.next_mode)
        elif self.next_transition_delay > 0:
            self.next_transition_delay = self.next_transition_delay - 1
            if self.next_transition_delay == 0:
                self.do_triggers(0, self.next_mode)

        if is_running_mode(self.mode):
            cycled = False
            new_iteration = self.iteration + 1
            if (new_iteration >= self.n_cycles):
                new_iteration = 0
                cycled = True
            self.iteration = new_iteration
            
            self.do_triggers(self.iteration+1, self.mode)

            if cycled:
                self._cycle_nr += 1
                self.cycledUnsafe.emit(self._cycle_nr)
                
        self._last_handled_source_cycle_nr = cycle_nr
        self.logger.trace(lambda: 'handle sync cycle done')
    
    def cancel_all(self):
        self.logger.trace(lambda: 'cancel_all')
        for loop in self._running_loops:
            loop.transition(LoopMode.Stopped.value, 0, DontAlignToSyncImmediately)
        self.running_loops = []

    def handle_transition(self, mode):
        self.logger.debug(lambda: f'handle transition to {mode}')
        self.next_transition_delay = -1
        if mode != self._mode:
            self.mode = mode
            if not is_running_mode(mode):
                self.iteration = 0

    def default_trigger_handler(self, loop, mode):
        if loop == self:
            # Don't queue the transition, apply it immediately
            self.transition_impl (mode, 0, DontAlignToSyncImmediately)
        else:
            # Queue the transition for next cycle. That ensures with nested composites that
            # composites always trigger themselves first, then get triggered by others.
            # This is desired (e.g. if a script wants to stop, but a composite wants it to
            # continue playing next cycle)
            QTimer.singleShot(0, lambda loop=loop, mode=mode: loop.transition(mode, 0, DontAlignToSyncImmediately))

    # In preparation for the given upcoming iteration, create the triggers for our child loops.
    # Instead of executing them directly, each trigger will call the callback with (loop, mode).
    # (the default callback is to execute the transition)
    def do_triggers(self, iteration, mode, trigger_callback = lambda self,loop,mode: CompositeLoop.default_trigger_handler(self, loop, mode), nested=False):
        schedule = self._schedule
        sched_keys = [int(k) for k in schedule.keys()]
        self.logger.debug(lambda: f'{self.kind} composite loop - do_triggers({iteration}, {mode})')
        if iteration in sched_keys:
            elem = schedule[str(iteration)]
            loops_end = elem['loops_end']
            loops_start = elem['loops_start']
            for loop in loops_end:
                self.logger.debug(lambda: f'loop end: {loop.instanceIdentifier}')
                trigger_callback(self, loop, LoopMode.Stopped.value)
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
                                        trigger_callback(self, loop, LoopMode.Stopped.value)
                                        self._running_loops.remove(loop)
                                        self.runningLoopsChanged.emit(self._running_loops)
                                        handled = True
                                        break

                        if handled:
                            continue

                    self.logger.debug(lambda: f'generate loop start: {loop.instanceIdentifier}')
                    trigger_callback(self, loop, loop_mode)
                    self._running_loops.add(loop)
                    self.runningLoopsChangedUnsafe.emit(self._running_loops)

        if (iteration >= self.n_cycles) and not nested:
            self.logger.debug(lambda: 'extra trigger for cycle end')
            if self.kind == 'script' and not (self.next_transition_delay >= 0 and is_running_mode(self.next_mode)):
                self.logger.debug(lambda: f'ending script {self.mode} {self.next_mode} {self.next_transition_delay}')
                trigger_callback(self, self, LoopMode.Stopped.value)
            elif is_recording_mode(self.mode):
                self.logger.debug(lambda: 'cycle: recording end')
                # Recording ends next cycle, transition to playing or stopped
                trigger_callback(self, self, (LoopMode.Playing.value if self._play_after_record else LoopMode.Stopped.value))
            else:
                self.logger.debug(lambda: 'cycling')
                # Will cycle around - trigger the actions for next cycle
                self.do_triggers(0, self.mode, trigger_callback, True)
    
    def maybe_initialize(self):
        if self._backend and self._backend.initialized and not self._initialized:
            self.logger.debug(lambda: 'Found backend, initializing')
            self.initializedChanged.emit(True)
    
    # Update from the back-end.
    @ShoopSlot(thread_protection = ThreadProtectionType.OtherThread)
    def updateOnOtherThread(self):
        if self._backend:
            for cycle_nr in self._pending_cycles:
                self.handle_sync_loop_trigger_impl(cycle_nr)
            for transition in self._pending_transitions:
                self.transition_impl(*transition)
            self._pending_transitions = []
            self._pending_cycles = []

    # Another loop which references this loop (composite) can notify this loop that it is
    # about to handle a sync loop cycle in advance, to ensure a deterministic ordering.
    @ShoopSlot(int, thread_protection = ThreadProtectionType.OtherThread)
    def dependent_will_handle_sync_loop_cycle(self, cycle_nr):
        self.handle_sync_loop_trigger_impl(cycle_nr)
    
