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
from .CompositeLoopBackend import CompositeLoopBackend

def iid(obj):
    if hasattr(obj, "instanceIdentifier"):
        return obj.instanceIdentifier
    else:
        return obj.property("instance_identifier")

def substitute_schedule_backend_loops(schedule):
    schedule_out = {}
    for (iteration,iteration_data) in schedule.items():
        def transform_list(l):
            o = list()
            for i in l:
                if isinstance(i, list):
                    o.append(transform_list(i))
                elif i:
                    o.append(i.get_backend_loop())
                else:
                    o.append(i)
            return o

        loops_start_out = transform_list(iteration_data['loops_start'])
        loops_end_out = transform_list(iteration_data['loops_end'])
        loops_ignored_out = transform_list(iteration_data['loops_ignored'])
        schedule_out[iteration] = {
            'loops_start': loops_start_out,
            'loops_end': loops_end_out,
            'loops_ignored': loops_ignored_out,
        }
        if 'loop_modes' in iteration_data.keys():
            loop_modes_out = dict()
            for (key, value) in iteration_data['loop_modes'].items():
                loop_modes_out[key.get_backend_loop()] = value
            schedule_out[iteration]['loop_modes'] = loop_modes_out

# Manage a back-end composite loop (GUI thread)
class CompositeLoop(ShoopQQuickItem):
    # FIXME marker
    def is_composite_loop(self):
        pass
    
    def __init__(self, parent=None):
        super(CompositeLoop, self).__init__(parent)
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

        self._backend_obj = CompositeLoopBackend()

        # Updates backend -> frontend
        self._backend_obj.nCyclesChanged.connect(self.on_backend_n_cycles_changed, Qt.QueuedConnection)
        self._backend_obj.syncLengthChanged.connect(self.on_backend_sync_length_changed, Qt.QueuedConnection)
        self._backend_obj.iterationChanged.connect(self.on_backend_iteration_changed, Qt.QueuedConnection)
        self._backend_obj.modeChanged.connect(self.on_backend_mode_changed, Qt.QueuedConnection)
        self._backend_obj.syncPositionChanged.connect(self.on_backend_sync_position_changed, Qt.QueuedConnection)
        self._backend_obj.nextModeChanged.connect(self.on_backend_next_mode_changed, Qt.QueuedConnection)
        self._backend_obj.nextTransitionDelayChanged.connect(self.on_backend_next_transition_delay_changed, Qt.QueuedConnection)
        self._backend_obj.runningLoopsChanged.connect(self.on_backend_running_loops_changed, Qt.QueuedConnection)
        self._backend_obj.lengthChanged.connect(self.on_backend_length_changed, Qt.QueuedConnection)
        self._backend_obj.positionChanged.connect(self.on_backend_position_changed, Qt.QueuedConnection)
        self._backend_obj.initializedChanged.connect(self.on_backend_initialized_changed, Qt.QueuedConnection)
        self._backend_obj.cycled.connect(self.cycled, Qt.QueuedConnection)
        
        # Updates frontend -> backend
        self.backend_set_kind.connect(self._backend_obj.set_kind, Qt.QueuedConnection)
        self.backend_set_sync_loop.connect(self._backend_obj.set_sync_loop, Qt.QueuedConnection)
        self.backend_set_backend.connect(self._backend_obj.set_backend, Qt.QueuedConnection)
        self.backend_set_schedule.connect(self._backend_obj.set_schedule, Qt.QueuedConnection)
        self.backend_set_play_after_record.connect(self._backend_obj.set_play_after_record, Qt.QueuedConnection)
        self.backend_set_sync_mode_active.connect(self._backend_obj.set_sync_mode_active, Qt.QueuedConnection)
        self.backend_set_instance_identifier.connect(self._backend_obj.set_instance_identifier, Qt.QueuedConnection)
        self.backend_transition.connect(self._backend_obj.transition, Qt.QueuedConnection)
        self.backend_adopt_ringbuffers.connect(self._backend_obj.adopt_ringbuffers, Qt.QueuedConnection)
    
    cycled = ShoopSignal(int)
    backend_set_kind = ShoopSignal(str)
    backend_set_sync_loop = ShoopSignal('QVariant')
    backend_set_backend = ShoopSignal('QVariant')
    backend_set_schedule = ShoopSignal('QVariant')
    backend_set_play_after_record = ShoopSignal(bool)
    backend_set_sync_mode_active = ShoopSignal(bool)
    backend_set_instance_identifier = ShoopSignal(str)
    backend_transition = ShoopSignal(int, int, int)
    backend_adopt_ringbuffers = ShoopSignal('QVariant', 'QVariant', 'QVariant', int)
    
    # backend (frontend -> backend)
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
            self.logger.debug(lambda: 'backend -> {}'.format(l))
            self.backend_set_backend.emit(l)
            self.backendChanged.emit(l)

    # schedule (frontend -> backend)
    # See CompositeLoop.qml for the format of this property.
    scheduleChanged = ShoopSignal('QVariant')
    @ShoopProperty('QVariant', notify=scheduleChanged)
    def schedule(self):
        return self._schedule
    @schedule.setter
    def schedule(self, val):
        self.logger.debug(lambda: f'set schedule')
        self._schedule = val
        self.update_backend_schedule()
        self.scheduleChanged.emit(val)
    def update_backend_schedule(self):
        # The schedule may arrive as a JSValue and loops in the schedule may be JSValues too
        pval = recursively_convert_jsvalue(self._schedule)
        # Replace loops by their back-end objects in the schedule before passing on
        pval = substitute_schedule_backend_loops(pval)
        self.backend_set_schedule.emit(pval)


    # initialized (backend -> frontend)
    initializedChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=initializedChanged)
    def initialized(self):
        return self._initialized
    @ShoopSlot(bool)
    def on_backend_initialized_changed(self, v):
        if v != self._initialized:
            self.logger.debug(lambda: f'initialized -> {v}')

            # TODO: why is this needed
            self.backend_set_instance_identifier.emit(self.instanceIdentifier)
            self.backend_set_kind.emit(self.kind)
            self.update_backend_sync_loop()
            self.backend_set_sync_mode_active.emit(self.sync_mode_active)
            self.backend_set_play_after_record.emit(self.play_after_record)
            self.update_backend_schedule()
            
            self._initialized = v
            self.initializedChanged.emit(v)
    
    # sync_loop (frontend -> backend)
    syncLoopChanged = ShoopSignal('QVariant')
    @ShoopProperty('QVariant', notify=syncLoopChanged)
    def sync_loop(self):
        return self._sync_loop
    @sync_loop.setter
    def sync_loop(self, val):
        if val != self._sync_loop:
            self.logger.debug(lambda: f'sync loop -> {val}')
            self._sync_loop = val
            self.update_backend_sync_loop()
            self.syncLoopChanged.emit(val)
    def update_backend_sync_loop(self):
        backend_sync_loop = self._sync_loop.get_backend_loop() if self._sync_loop else None
        self.backend_set_sync_loop.emit(backend_sync_loop)
    
    # running_loops (backend -> frontend)
    runningLoopsChanged = ShoopSignal('QVariant')
    @ShoopProperty('QVariant', notify=runningLoopsChanged)
    def running_loops(self):
        return self._running_loops
    @ShoopSlot('QVariant')
    def on_backend_running_loops_changed(self, v):
        self._running_loops = [l.get_frontend_loop() for l in v]
        self.logger.debug(lambda: f'running loops -> {self._running_loops}')
        self.runningLoopsChanged.emit(v)

    # iteration (backend -> frontend)
    iterationChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=iterationChanged)
    def iteration(self):
        return self._iteration
    @ShoopSlot(int)
    def on_backend_iteration_changed(self, v):
        if self._iteration != v:
            self.logger.debug(lambda: f'iteration -> {v}')
            self._iteration = v
            self.iterationChanged.emit(v)

    # play_after_record (frontend -> backend)
    playAfterRecordChanged = ShoopSignal(bool)
    @ShoopProperty(int, notify=playAfterRecordChanged)
    def play_after_record(self):
        return self._play_after_record
    @play_after_record.setter
    def play_after_record(self, val):
        if self._play_after_record != val:
            self.logger.debug(lambda: f'play after record -> {val}')
            self._play_after_record = val
            self.backend_set_play_after_record.emit(val)
            self.playAfterRecordChanged.emit(val)
    
    # sync_mode_active (frontend -> backend)
    syncModeActiveChanged = ShoopSignal(bool)
    @ShoopProperty(int, notify=syncModeActiveChanged)
    def sync_mode_active(self):
        return self._sync_mode_active
    @sync_mode_active.setter
    def sync_mode_active(self, val):
        if self._sync_mode_active != val:
            self.logger.debug(lambda: f'sync mode active -> {val}')
            self._sync_mode_active = val
            self.backend_set_sync_mode_active.emit(val)
            self.syncModeActiveChanged.emit(val)

    # mode (backend -> frontend)
    modeChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=modeChanged)
    def mode(self):
        return self._mode
    @ShoopSlot(int)
    def on_backend_mode_changed(self, v):
        if self._mode != v:
            self.logger.debug(lambda: f'mode -> {v}')
            self._mode = v
            self.modeChanged.emit(v)
    
    # next_mode (backend -> frontend)
    nextModeChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=nextModeChanged)
    def next_mode(self):
        return self._next_mode
    @ShoopSlot(int)
    def on_backend_next_mode_changed(self, v):
        if self._next_mode != v:
            self.logger.debug(lambda: f'next mode -> {v}')
            self._next_mode = v
            self.nextModeChanged.emit(v)
    
    # next_transition_delay (backend -> frontend)
    nextTransitionDelayChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=nextTransitionDelayChanged)
    def next_transition_delay(self):
        return self._next_transition_delay
    @ShoopSlot(int)
    def on_backend_next_transition_delay_changed(self, v):
        if self._next_transition_delay != v:
            self.logger.debug(lambda: f'next transition delay -> {v}')
            self._next_transition_delay = v
            self.nextTransitionDelayChanged.emit(v)

    # n_cycles (backend -> frontend)
    nCyclesChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=nCyclesChanged)
    def n_cycles(self):
        return self._n_cycles
    @ShoopSlot(int)
    def on_backend_n_cycles_changed(self, v):
        if self._n_cycles != v:
            self.logger.debug(lambda: f'n cycles -> {v}')
            self._n_cycles = v
            self.nCyclesChanged.emit(v)

    # length (backend -> frontend)
    lengthChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=lengthChanged)
    def length(self):
        return self._length
    @ShoopSlot(int)
    def on_backend_length_changed(self, v):
        if self._length != v:
            self.logger.debug(lambda: f'length -> {v}')
            self._length = v
            self.lengthChanged.emit(v)

    # kind (frontend -> backend)
    kindChanged = ShoopSignal(str)
    @ShoopProperty(str, notify=kindChanged)
    def kind(self):
        return self._kind
    @kind.setter
    def kind(self, val):
        if val != self._kind:
            self.logger.debug(lambda: f'kind -> {val}')
            self._kind = val
            self.backend_set_kind.emit(val)
            self.kindChanged.emit(self._kind)
    
    # sync_position (backend -> frontend)
    syncPositionChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=syncPositionChanged)
    def sync_position(self):
        return self._sync_position
    @ShoopSlot(int)
    def on_backend_sync_position_changed(self, v):
        if self._sync_position != v:
            self.logger.debug(lambda: f'sync position -> {v}')
            self._sync_position = v
            self.syncPositionChanged.emit(v)

    # instance identifier (frontend -> backend)
    instanceIdentifierChanged = ShoopSignal(str)
    @ShoopProperty(str, notify=instanceIdentifierChanged)
    def instanceIdentifier(self):
        return self.logger.instanceIdentifier
    @instanceIdentifier.setter
    def instanceIdentifier(self, l):
        if l and l != self.logger.instanceIdentifier:
            self.logger.debug(lambda: f'instance identifier -> {l}')
            self.logger.instanceIdentifier = l
            self.backend_set_instance_identifier.emit(l)
            self.instanceIdentifierChanged.emit(l)
    
    # sync_length (backend -> frontend)
    syncLengthChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=syncLengthChanged)
    def sync_length(self):
        return self._sync_length
    @ShoopSlot(int)
    def on_backend_sync_length_changed(self, v):
        if self._sync_length != v:
            self.logger.debug(lambda: f'sync length -> {v}')
            self._sync_length = v
            self.syncLengthChanged.emit(v)

    # position (backend -> frontend)
    positionChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=positionChanged)
    def position(self):
        return self._position
    @ShoopSlot(int)
    def on_backend_position_changed(self, v):
        if self._position != v:
            self.logger.debug(lambda: f'position -> {v}')
            self._position = v
            self.positionChanged.emit(v)
    
    @ShoopSlot(int, int, int)
    def transition(self, mode, maybe_delay, maybe_to_sync_at_cycle):
        self.logger.trace(lambda: f'queue transition -> {mode} : wait {maybe_delay}, align @ {maybe_to_sync_at_cycle}')
        self.backend_transition.emit(mode, maybe_delay, maybe_to_sync_at_cycle)

    @ShoopSlot('QVariant', 'QVariant', 'QVariant', int)
    def adopt_ringbuffers(self, reverse_start_cycle, cycles_length, go_to_cycle, go_to_mode):
        self.logger.debug(lambda: f'adopt ringbuffers')
        self.backend_adopt_ringbuffers.emit(reverse_start_cycle, cycles_length, go_to_cycle, go_to_mode)
    
