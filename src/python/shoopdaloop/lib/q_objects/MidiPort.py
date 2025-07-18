import re
import time
import os
import tempfile
import json
from typing import *
import sys

from PySide6.QtCore import Qt, QObject, Signal, Property, Slot, QTimer, SIGNAL, SLOT
from PySide6.QtQuick import QQuickItem

from .Port import Port
from .ShoopPyObject import *

from ..backend_wrappers import midi_msgs_list_to_backend, midi_msgs_list_from_backend

import shoop_py_backend
from ..findFirstParent import findFirstParent
from ..findChildItems import findChildItems
from ..logging import Logger

# Wraps a back-end port.
class MidiPort(Port):
    def __init__(self, parent=None):
        super(MidiPort, self).__init__(parent)
        self._n_input_events = self._new_n_input_events =0
        self._n_input_notes_active = self._new_n_input_notes_active = 0
        self._n_output_events = self._new_n_output_events =0
        self._n_output_notes_active = self._new_n_output_notes_active =0
        self._n_updates_pending = 0
        self.logger = Logger('Frontend.MidiPort')
        
        self._signal_sender = ThreadUnsafeSignalEmitter()
        self._signal_sender.signal.connect(self.updateOnGuiThread, Qt.QueuedConnection)

    ######################
    # PROPERTIES
    ######################

    # Number of events triggered since last update (input)
    nInputEventsChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=nInputEventsChanged)
    def n_input_events(self):
        return self._n_input_events
    @n_input_events.setter
    def n_input_events(self, s):
        if self._n_input_events != s:
            self._n_input_events = s
            self.nInputEventsChanged.emit(s)
    
    # Number of notes currently being played (input)
    nInputNotesActiveChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=nInputNotesActiveChanged)
    def n_input_notes_active(self):
        return self._n_input_notes_active
    @n_input_notes_active.setter
    def n_input_notes_active(self, s):
        if self._n_input_notes_active != s:
            self._n_input_notes_active = s
            self.nInputNotesActiveChanged.emit(s)
    
    # Number of events triggered since last update (output)
    nOutputEventsChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=nOutputEventsChanged)
    def n_output_events(self):
        return self._n_output_events
    @n_output_events.setter
    def n_output_events(self, s):
        if self._n_output_events != s:
            self._n_output_events = s
            self.nOutputEventsChanged.emit(s)
    
    # Number of notes currently being played (output)
    nOutputNotesActiveChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=nOutputNotesActiveChanged)
    def n_output_notes_active(self):
        return self._n_output_notes_active
    @n_output_notes_active.setter
    def n_output_notes_active(self, s):
        if self._n_output_notes_active != s:
            self._n_output_notes_active = s
            self.nOutputNotesActiveChanged.emit(s)
    
    ###########
    ## SLOTS
    ###########

    # Update mode from the back-end.
    @ShoopSlot(thread_protection = ThreadProtectionType.OtherThread)
    def updateOnOtherThread(self):
        self.logger.trace(lambda: f'update on back-end thread (initialized {self._initialized})')
        if not self._initialized:
            return
        state = self._backend_obj.get_state()
        self._new_n_input_events = state.n_input_events
        self._new_n_input_notes_active = state.n_input_notes_active
        self._new_n_output_events = state.n_output_events
        self._new_n_output_notes_active = state.n_output_notes_active
        self._new_name = state.name
        self._new_muted = state.muted
        self._new_passthrough_muted = state.passthrough_muted
        self._n_ringbuffer_samples = state.ringbuffer_n_samples
        self._n_updates_pending += 1

        self._signal_sender.do_emit()
    
    @ShoopSlot()
    def updateOnGuiThread(self):
        self.logger.trace(lambda: f'update on GUI thread (# {self._n_updates_pending}, initialized {self._initialized})')
        if not self._initialized or not self.isValid():
            return
        if self._n_updates_pending == 0:
            return
        self.n_input_events = self._new_n_input_events
        self.n_input_notes_active = self._new_n_input_notes_active
        self.n_output_events = self._new_n_output_events
        self.n_output_notes_active = self._new_n_output_notes_active
        self.name = self._new_name
        self.muted = self._new_muted
        self.passthrough_muted = self._new_passthrough_muted
        self.n_ringbuffer_samples = self._n_ringbuffer_samples
        self._n_updates_pending = 0
    
    @ShoopSlot(list)
    def dummy_queue_msgs(self, msgs):
        converted = midi_msgs_list_to_backend(msgs)
        self._backend_obj.dummy_queue_msgs(converted)
    
    @ShoopSlot(result=list)
    def dummy_dequeue_data(self):
        return midi_msgs_list_from_backend(self._backend_obj.dummy_dequeue_data())
    
    @ShoopSlot(int)
    def dummy_request_data(self, n_frames):
        self._backend_obj.dummy_request_data(n_frames)
    
    @ShoopSlot()
    def dummy_clear_queues(self):
        self._backend_obj.dummy_clear_queues()
    
    ##########
    ## INTERNAL MEMBERS
    ##########
    
    def get_data_type(self):
        return int(shoop_py_backend.PortDataType.Midi)
    
    def maybe_initialize_internal(self, name_hint, input_connectability, output_connectability):
        # Internal ports are owned by FX chains.
        maybe_fx_chain = findFirstParent(self, lambda p: p and isinstance(p, QQuickItem) and p.inherits('FXChain'))
        if maybe_fx_chain and not self._backend_obj:
            if not maybe_fx_chain.initialized:
                maybe_fx_chain.initializedChanged.connect(lambda: self.maybe_initialize())
            else:
                # Determine our index in the FX chain
                def find_index():
                    idx = 0
                    for port in findChildItems(maybe_fx_chain, lambda i: i.inherits('MidiPort')):
                        if port == self:
                            return idx
                        elif port.input_connectability == self.input_connectability and port.output_connectability == self.output_connectability:
                            idx += 1
                    return None
                idx = find_index()
                if idx == None:
                    raise Exception('Could not find self in FX chain')
                # Now request our backend object.
                n_ringbuffer = self.n_ringbuffer_samples
                if not (self.output_connectability & int(shoop_py_backend.PortConnectabilityKind.Internal)):
                    self._backend_obj = maybe_fx_chain.get_backend_obj().get_midi_input_port(idx)
                    self.push_state()
                    self.set_min_n_ringbuffer_samples (n_ringbuffer)
                    self.connect_backend_updates()
                else:
                    raise Exception('Input ports (FX outputs) of MIDI type not supported')

    def maybe_initialize_external(self, name_hint, input_connectability, output_connectability):
        if self._backend_obj:
            return # never initialize more than once
        direction = int(shoop_py_backend.PortDirection.Input) if not (input_connectability & int(shoop_py_backend.PortConnectabilityKind.Internal)) else int(shoop_py_backend.PortDirection.Output)
        from shoop_rust import shoop_rust_open_driver_midi_port
        from shiboken6 import getCppPointer
        self._backend_obj = shoop_rust_open_driver_midi_port(
            getCppPointer(self._backend)[0],
            name_hint,
            direction,
            self.n_ringbuffer_samples
        )
        self.logger.trace(lambda: f'backend_obj = {self._backend_obj}')
        self.push_state()
        self.connect_backend_updates()

    def maybe_initialize_impl(self, name_hint, input_connectability, output_connectability, is_internal):
        if is_internal:
            self.maybe_initialize_internal(name_hint, input_connectability, output_connectability)
        else:
            self.maybe_initialize_external(name_hint, input_connectability, output_connectability)
            
    def connect_backend_updates(self):
        QObject.connect(self._backend, SIGNAL("updated_on_gui_thread()"), self, SLOT("updateOnGuiThread()"), Qt.DirectConnection)
        QObject.connect(self._backend, SIGNAL("updated_on_backend_thread()"), self, SLOT("updateOnOtherThread()"), Qt.DirectConnection)
    
    def push_state(self):
        self.set_muted(self.muted)
        self.set_passthrough_muted(self.passthrough_muted)