from typing import *

from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer, QMetaObject, Qt, SIGNAL, SLOT, Q_ARG
from PySide6.QtQuick import QQuickItem
from PySide6.QtQml import QJSValue

from .ShoopPyObject import *

from ..backend_wrappers import DontWaitForSync, DontAlignToSyncImmediately

from ..mode_helpers import is_playing_mode, is_running_mode, is_recording_mode
from ..recursive_jsvalue_convert import recursively_convert_jsvalue
from ..q_objects.Logger import Logger

from collections.abc import Mapping, Sequence

from ..loop_helpers import transition_loop, transition_loops, loop_adopt_ringbuffers

import traceback
import math

import shoop_py_backend

def iid(obj):
    if hasattr(obj, "instanceIdentifier"):
        return obj.instanceIdentifier
    else:
        return obj.property("instance_identifier")
    
class CompositeLoopBackend(ShoopQObject):
    # FIXME marker
    def is_composite_loop(self):
        pass
    
    def __init__(self, parent=None):
        super(CompositeLoopBackend, self).__init__(parent)
        self._schedule = {}
        self.logger = Logger(self)
        self.logger.name = 'Frontend.CompositeLoop'
        self._sync_loop = None
        self._running_loops = set()
        self._iteration = 0
        self._backend = None
        self._mode = int(shoop_py_backend.LoopMode.Stopped)
        self._next_mode = int(shoop_py_backend.LoopMode.Stopped)
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
        
        self.syncLoopChanged.connect(self.update_sync_position, Qt.QueuedConnection)
        self.syncLoopChanged.connect(self.update_sync_length, Qt.QueuedConnection)

        self.iterationChanged.connect(self.update_position, Qt.DirectConnection)
        self.modeChanged.connect(self.update_position, Qt.DirectConnection)

        def on_backend_changed(backend):
            self.logger.debug(lambda: 'Backend changed')
            QObject.connect(backend, SIGNAL("readyChanged()"), self, SLOT("maybe_initialize()"), Qt.QueuedConnection)
            self.maybe_initialize()
        self.backendChanged.connect(lambda b: on_backend_changed(b))
    
    cycled = ShoopSignal(int)

    @ShoopSlot(str)
    def set_kind(self, v):
        self.kind = v
    
    @ShoopSlot('QVariant')
    def set_sync_loop(self, v):
        self.sync_loop = v
    
    @ShoopSlot('QVariant')
    def set_backend(self, v):
        self.backend = v
    
    @ShoopSlot('QVariant')
    def set_schedule(self, v):
        self.schedule = v
    
    @ShoopSlot(bool)
    def set_play_after_record(self, v):
        self.play_after_record = v
    
    @ShoopSlot(bool)
    def set_sync_mode_active(self, v):
        self.sync_mode_active = v
    
    @ShoopSlot(str)
    def set_instance_identifier(self, v):
        self.instanceIdentifier = v + '-backend'

    ######################
    ## PROPERTIES
    ######################
    
    # backend
    backendChanged = ShoopSignal('QVariant')
    @ShoopProperty('QVariant', notify=backendChanged)
    def backend(self):
        return self._backend
    @backend.setter
    def backend(self, l):
        if l and l != self._backend:
            if self._backend:
                self.logger.throw_error('May not change backend of existing loop')
            self._backend = l
            self.logger.trace(lambda: 'Set backend -> {}'.format(l))
            self.backendChanged.emit(l)
            self.maybe_initialize()

    # schedule
    # See CompositeLoop.qml for the format of this property.
    scheduleChanged = ShoopSignal('QVariant')
    @ShoopProperty('QVariant', notify=scheduleChanged)
    def schedule(self):
        return self._schedule
    @schedule.setter
    def schedule(self, val):
        def stringify_schedule (schedule):            
            rval = ''
            for iteration,elem in schedule.items():
                rval += f'{iteration}:'
                for to_stop in elem['loops_end']:
                    rval += f'\n- {iid(to_stop)} -> Stopped'
                for to_start in elem['loops_start']:
                    rval += f'\n- {iid(to_start[0])} -> {to_start[1] if to_start[1] != None else "Autostart"}'
                for to_ignore in elem['loops_ignored']:
                    rval += f'\n- {iid(to_ignore)} ignored'
                rval += "\n"
            return rval

        self.logger.trace(lambda: f'schedule updated:\n{stringify_schedule(val)}')
        self._schedule = val
        self.scheduleChanged.emit(self._schedule)
        n = (max([int(k) for k in self._schedule.keys()]) if self._schedule else 0)
        if n != self._n_cycles:
            self.logger.debug(lambda: f'n_cycles -> {n}')
            self._n_cycles = n
            self.nCyclesChanged.emit(n)
            self.update_length()

    # initialized
    initializedChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=initializedChanged)
    def initialized(self):
        return self._initialized
    
    # sync_loop
    syncLoopChanged = ShoopSignal('QVariant')
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
                self.logger.debug(lambda: 'connecting to sync loop')
                QObject.connect(val, SIGNAL("positionChanged()"), self, SLOT("update_sync_position()"), Qt.DirectConnection)
                QObject.connect(val, SIGNAL("lengthChanged()"), self, SLOT("update_sync_length()"), Qt.DirectConnection)
                QObject.connect(val, SIGNAL("cycled(int)"), self, SLOT("handle_sync_loop_trigger(int)"), Qt.DirectConnection)
                self.update_sync_position()
                self.update_sync_length()
            self.syncLoopChanged.emit(self._sync_loop)
    
    # running_loops
    runningLoopsChanged = ShoopSignal('QVariant')
    @ShoopProperty('QVariant', notify=runningLoopsChanged)
    def running_loops(self):
        return list(self._running_loops)
    @running_loops.setter
    def running_loops(self, val):
        self._running_loops = set(val)
        self.runningLoopsChanged.emit(self._running_loops)

    # iteration
    iterationChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=iterationChanged)
    def iteration(self):
        return self._iteration
    @iteration.setter
    def iteration(self, val):
        if self._iteration != val:
            self.logger.debug(lambda: f'iteration -> {val}')
            self._iteration = val
            self.iterationChanged.emit(self._iteration)

    # play_after_record
    playAfterRecordChanged = ShoopSignal(bool)
    @ShoopProperty(int, notify=playAfterRecordChanged)
    def play_after_record(self):
        return self._play_after_record
    @play_after_record.setter
    def play_after_record(self, val):
        if self._play_after_record != val:
            self.logger.debug(lambda: f'play after record -> {val}')
            self._play_after_record = val
            self.playAfterRecordChanged.emit(self._play_after_record)
    
    # sync_mode_active
    syncModeActiveChanged = ShoopSignal(bool)
    @ShoopProperty(int, notify=syncModeActiveChanged)
    def sync_mode_active(self):
        return self._sync_mode_active
    @sync_mode_active.setter
    def sync_mode_active(self, val):
        if self._sync_mode_active != val:
            self.logger.debug(lambda: f'sync mode active -> {val}')
            self._sync_mode_active = val
            self.syncModeActiveChanged.emit(self._sync_mode_active)

    # mode
    modeChanged = ShoopSignal(int)
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
    nextModeChanged = ShoopSignal(int)
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
    nextTransitionDelayChanged = ShoopSignal(int)
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
    nCyclesChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=nCyclesChanged)
    def n_cycles(self):
        return self._n_cycles

    # length
    lengthChanged = ShoopSignal(int)
    @ShoopSlot()
    def update_length(self, *args):
        self.logger.trace(lambda: 'updating length')
        v = self._sync_length * self._n_cycles
        if v != self._length:
            self.logger.trace(lambda: f'length -> {v}')
            self._length = v
            self.lengthChanged.emit(v)
    @ShoopProperty(int, notify=lengthChanged)
    def length(self):
        return self._length

    # kind
    kindChanged = ShoopSignal(str)
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
    syncPositionChanged = ShoopSignal(int)

    # instanceIdentifier (for clarity in debugging)
    instanceIdentifierChanged = ShoopSignal(str)
    @ShoopProperty(str, notify=instanceIdentifierChanged)
    def instanceIdentifier(self):
        return self.logger.instanceIdentifier
    @instanceIdentifier.setter
    def instanceIdentifier(self, l):
        if l and l != self.logger.instanceIdentifier:
            self.logger.debug(lambda: f'instance identifier -> {l}')
            self.logger.instanceIdentifier = l
            self.instanceIdentifierChanged.emit(l)
    
    @ShoopSlot()
    def update_sync_position(self, *args):
        self.logger.trace(lambda: 'updating sync position')
        v = 0
        if self.sync_loop:
            v = self.sync_loop.property('position')
        if v != self._sync_position:
            self.logger.trace(lambda: f'sync_position -> {v}')
            self._sync_position = v
            self.syncPositionChanged.emit(v)
            self.update_position()
    @ShoopProperty(int, notify=syncPositionChanged)
    def sync_position(self):
        return self._sync_position
    
    # sync_length
    syncLengthChanged = ShoopSignal(int)
    
    @ShoopSlot()
    def update_sync_length(self, *args):
        self.logger.trace(lambda: 'updating sync length')
        v = 0
        if self.sync_loop:
            v = self.sync_loop.property('length')
        if v != self._sync_length:
            self.logger.trace(lambda: f'sync_length -> {v}')
            self._sync_length = v
            self.syncLengthChanged.emit(v)
            self.update_length()
            self.update_position()
    @ShoopProperty(int, notify=syncLengthChanged)
    def sync_length(self):
        return self._sync_length

    # position
    positionChanged = ShoopSignal(int)
    @ShoopSlot()
    def update_position(self, *args):
        self.logger.trace(lambda: 'updating position')
        v = max(0, self._iteration) * self._sync_length
        if is_running_mode(self.mode):
            v += self._sync_position
        if v != self._position:
            self.logger.trace(lambda: f'position -> {v}')
            self._position = v
            self.positionChanged.emit(v)
    @ShoopProperty(int, notify=positionChanged)
    def position(self):
        return self._position
    
    @ShoopSlot(int, int, int)
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
                    str = str + f'\n - iteration {iteration}: {iid(t["loop"])} -> {t["mode"]}'
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
            if loop.property('sync_source'):
                n_cycles = math.ceil(loop.property('length') / loop.property('sync_source').property('length'))
            mode = elem['mode']
            
            current_cycle = None
            if t['mode'] in [int(shoop_py_backend.LoopMode.Playing), int(shoop_py_backend.LoopMode.Replacing), int(shoop_py_backend.LoopMode.PlayingDryThroughWet), int(shoop_py_backend.LoopMode.RecordingDryIntoWet)]:
                # loop around
                current_cycle = n_cycles_ago % max(n_cycles, 1)
            elif t['mode'] == int(shoop_py_backend.LoopMode.Recording):
                # keep going indefinitely
                current_cycle = n_cycles_ago
            
            self.logger.trace(lambda: f'loop {iid(loop)} -> {mode}, goto cycle {current_cycle} ({elem["mode"]} triggered {n_cycles_ago} cycles ago, loop length {n_cycles} cycles)')
            transition_loop(loop,
                            mode,
                            DontWaitForSync,
                            (current_cycle if mode != int(shoop_py_backend.LoopMode.Stopped) else DontAlignToSyncImmediately))
        
        # Apply our own mode change
        self.mode = mode
        self.iteration = sync_cycle
        self.update_length()
        self.update_position()
        self.logger.trace(lambda: f'immediate sync: Done - mode -> {mode}, iteration -> {sync_cycle}, position -> {self._position}')

        # Perform the trigger(s) for the next loop cycle
        self.do_triggers(self.iteration + 1, mode)
    
    @ShoopSlot(int)
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
        transitions = self.list_transitions(int(shoop_py_backend.LoopMode.Recording), 0, self._n_cycles)

        if self.logger.should_trace():
            str = 'ringbuffer grab - virtual transition list:'
            for iteration,ts in transitions.items():
                for t in ts:
                    str = str + f'\n - iteration {iteration}: {iid(t["loop"])} -> {t["mode"]}'
            self.logger.trace(lambda: str)
        
        # Find the first recording range for each loop.
        loop_recording_starts = {}
        loop_recording_ends = {}
        for it,ts in transitions.items():
            for t in ts:
                if t['mode'] == int(shoop_py_backend.LoopMode.Recording):
                    # Store only the first recording start.
                    loop_recording_starts[t['loop']] = min(
                        it,
                        loop_recording_starts[t['loop']]
                    ) if t['loop'] in loop_recording_starts.keys() else it
        for it,ts in transitions.items():
            for t in ts:
                if t['mode'] != int(shoop_py_backend.LoopMode.Recording) and t['loop'] in loop_recording_starts.keys() and it > loop_recording_starts[t['loop']]:
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
            self.logger.trace(f"to grab: {iid(loop)} @ reverse start {reverse_start_offset}, n = {grab_n}")

        for item in to_grab:
            loop_adopt_ringbuffers(item['loop'], item['reverse_start'], item['n'], 0, int(shoop_py_backend.LoopMode.Unknown))
        
        if go_to_mode != int(shoop_py_backend.LoopMode.Unknown):
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
            if hasattr(l, "dependent_will_handle_sync_loop_cycle"):
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
        transition_loops(self._running_loops,
                         int(shoop_py_backend.LoopMode.Stopped),
                         0,
                         DontAlignToSyncImmediately)
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
            QTimer.singleShot(0, lambda loop=loop, mode=mode: transition_loop(loop, mode, 0, DontAlignToSyncImmediately))

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
                self.logger.debug(lambda: f'loop end: {iid(loop)}')
                trigger_callback(self, loop, int(shoop_py_backend.LoopMode.Stopped))
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
                                        trigger_callback(self, loop, int(shoop_py_backend.LoopMode.Stopped))
                                        self._running_loops.remove(loop)
                                        self.runningLoopsChanged.emit(self._running_loops)
                                        handled = True
                                        break

                        if handled:
                            continue

                    self.logger.debug(lambda: f'generate loop start: {iid(loop)}')
                    trigger_callback(self, loop, loop_mode)
                    self._running_loops.add(loop)
                    self.runningLoopsChanged.emit(self._running_loops)

        if (iteration >= self.n_cycles) and not nested:
            self.logger.debug(lambda: 'extra trigger for cycle end')
            if self.kind == 'script' and not (self.next_transition_delay >= 0 and is_running_mode(self.next_mode)):
                self.logger.debug(lambda: f'ending script {self.mode} {self.next_mode} {self.next_transition_delay}')
                trigger_callback(self, self, int(shoop_py_backend.LoopMode.Stopped))
            elif is_recording_mode(self.mode):
                self.logger.debug(lambda: 'cycle: recording end')
                # Recording ends next cycle, transition to playing or stopped
                trigger_callback(self, self, (int(shoop_py_backend.LoopMode.Playing) if self._play_after_record else int(shoop_py_backend.LoopMode.Stopped)))
        else:
                self.logger.debug(lambda: 'cycling')
                # Will cycle around - trigger the actions for next cycle
                self.do_triggers(0, self.mode, trigger_callback, True)
    
    def connect_backend_updates(self):
        pass
        #QObject.connect(self._backend, SIGNAL("updated_on_backend_thread()"), self, SLOT("update()"), Qt.DirectConnection)
    
    def maybe_initialize(self):
        if self._backend and self._backend.property('ready') and not self._initialized:
            self.logger.debug(lambda: 'Found backend, initializing')
            # if not self.moveToThread(self._backend.get_backend_thread()):
            #     self.logger.error("Unable to move to back-end thread")
            self.connect_backend_updates()
            self._initialized = True
            self.initializedChanged.emit(True)
    
    # Update from the back-end.
    @ShoopSlot()
    def update(self):
        if self._backend:
            for cycle_nr in self._pending_cycles:
                self.handle_sync_loop_trigger_impl(cycle_nr)
            for transition in self._pending_transitions:
                self.transition_impl(*transition)
            self._pending_transitions = []
            self._pending_cycles = []

    # Another loop which references this loop (composite) can notify this loop that it is
    # about to handle a sync loop cycle in advance, to ensure a deterministic ordering.
    @ShoopSlot(int)
    def dependent_will_handle_sync_loop_cycle(self, cycle_nr):
        self.handle_sync_loop_trigger_impl(cycle_nr)