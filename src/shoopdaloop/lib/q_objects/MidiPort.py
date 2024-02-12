import re
import time
import os
import tempfile
import json
from typing import *
import sys

from PySide6.QtCore import Qt, QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from .Port import Port
from .ShoopPyObject import *

from ..backend_wrappers import PortDirection
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
        
        self._signal_sender = SingleSignalObject()
        self._signal_sender.signal.connect(self.updateOnGuiThread, Qt.QueuedConnection)

    ######################
    # PROPERTIES
    ######################

    # Number of events triggered since last update (input)
    nInputEventsChanged = Signal(int)
    @ShoopProperty(int, notify=nInputEventsChanged)
    def n_input_events(self):
        return self._n_input_events
    @n_input_events.setter
    def n_input_events(self, s):
        if self._n_input_events != s:
            self._n_input_events = s
            self.nInputEventsChanged.emit(s)
    
    # Number of notes currently being played (input)
    nInputNotesActiveChanged = Signal(int)
    @ShoopProperty(int, notify=nInputNotesActiveChanged)
    def n_input_notes_active(self):
        return self._n_input_notes_active
    @n_input_notes_active.setter
    def n_input_notes_active(self, s):
        if self._n_input_notes_active != s:
            self._n_input_notes_active = s
            self.nInputNotesActiveChanged.emit(s)
    
    # Number of events triggered since last update (output)
    nOutputEventsChanged = Signal(int)
    @ShoopProperty(int, notify=nOutputEventsChanged)
    def n_output_events(self):
        return self._n_output_events
    @n_output_events.setter
    def n_output_events(self, s):
        if self._n_output_events != s:
            self._n_output_events = s
            self.nOutputEventsChanged.emit(s)
    
    # Number of notes currently being played (output)
    nOutputNotesActiveChanged = Signal(int)
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
    @ShoopSlot(thread_protected = False)
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
        self._n_updates_pending = 0
    
    @ShoopSlot(list)
    def dummy_queue_msgs(self, msgs):
        self._backend_obj.dummy_queue_msgs(msgs)
    
    @ShoopSlot(result=list)
    def dummy_dequeue_data(self):
        return self._backend_obj.dummy_dequeue_data()
    
    @ShoopSlot(int)
    def dummy_request_data(self, n_frames):
        self._backend_obj.dummy_request_data(n_frames)
    
    @ShoopSlot()
    def dummy_clear_queues(self):
        self._backend_obj.dummy_clear_queues()
    
    ##########
    ## INTERNAL MEMBERS
    ##########
    
    def maybe_initialize_internal(self, name_hint, direction):
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
                        elif port.direction == self.direction:
                            idx += 1
                    return None
                idx = find_index()
                if idx == None:
                    raise Exception('Could not find self in FX chain')
                # Now request our backend object.
                if self.direction == PortDirection.Output.value:
                    self._backend_obj = self.backend.get_backend_session_obj().get_fx_chain_midi_input_port(
                        maybe_fx_chain.get_backend_obj(),
                        idx
                    )
                    self.push_state()
                else:
                    raise Exception('Input ports (FX outputs) of MIDI type not supported')

    def maybe_initialize_external(self, name_hint, direction):
        if self._backend_obj:
            return # never initialize more than once
        self._backend_obj = self.backend.open_midi_port(name_hint, direction)
        self.push_state()

    def maybe_initialize_impl(self, name_hint, direction, is_internal):
        if is_internal:
            self.maybe_initialize_internal(name_hint, direction)
        else:
            self.maybe_initialize_external(name_hint, direction)
    
    def push_state(self):
        self.set_muted(self.muted)
        self.set_passthrough_muted(self.passthrough_muted)