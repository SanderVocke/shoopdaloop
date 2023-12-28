import re
import time
import os
import tempfile
import json
from typing import *
import sys

from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from .ShoopPyObject import *
from .Port import Port

from ..backend_wrappers import PortDirection
from ..findFirstParent import findFirstParent
from ..findChildItems import findChildItems
from ..logging import Logger

# Wraps a back-end port.
class AudioPort(Port):
    def __init__(self, parent=None):
        super(AudioPort, self).__init__(parent)
        self._input_peak = 0.0
        self._output_peak = 0.0
        self._gain = 1.0
        self.logger = Logger("Frontend.AudioPort")

    ######################
    # PROPERTIES
    ######################

    # input_peak
    inputPeakChanged = Signal(float)
    @Property(float, notify=inputPeakChanged)
    def input_peak(self):
        return self._input_peak
    @input_peak.setter
    def input_peak(self, s):
        if self._input_peak != s:
            self._input_peak = s
            self.inputPeakChanged.emit(s)
    
    # output_peak
    outputPeakChanged = Signal(float)
    @Property(float, notify=outputPeakChanged)
    def output_peak(self):
        return self._output_peak
    @output_peak.setter
    def output_peak(self, s):
        if self._output_peak != s:
            self._output_peak = s
            self.outputPeakChanged.emit(s)
    
    # gain
    gainChanged = Signal(float)
    @Property(float, notify=gainChanged)
    def gain(self):
        return self._gain
    @gain.setter
    def gain(self, s):
        if self._gain != s:
            self._gain = s
            self.gainChanged.emit(s)
    
    ###########
    ## SLOTS
    ###########

    # Update mode from the back-end.
    @Slot()
    def update(self):
        if not self.initialized:
            return
        state = self._backend_obj.get_state()
        self.input_peak = state.input_peak
        self.output_peak = state.output_peak
        self.name = state.name
        self.gain = state.gain
        self.muted = state.muted
        self.passthrough_muted = state.passthrough_muted
    
    @Slot(float)
    def set_gain(self, gain):
        if self._backend_obj:
            self._backend_obj.set_gain(gain)
    
    @Slot(list)
    def dummy_queue_data(self, data):
        self._backend_obj.dummy_queue_data(data)
    
    @Slot(int, result=list)
    def dummy_dequeue_data(self, n):
        return self._backend_obj.dummy_dequeue_data(n)
    
    @Slot(int)
    def dummy_request_data(self, n):
        self._backend_obj.dummy_request_data(n)

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
                    for port in findChildItems(maybe_fx_chain, lambda i: i.inherits('AudioPort')):
                        if port == self:
                            return idx
                        elif port.direction == self.direction:
                            idx += 1
                    return None
                idx = find_index()
                if idx == None:
                    self.logger.throw_error('Could not find self in FX chain')
                # Now request our backend object.
                if direction == PortDirection.Input.value:
                    self._backend_obj = self.backend.get_backend_session_obj().get_fx_chain_audio_output_port(
                        maybe_fx_chain.get_backend_obj(),
                        idx
                    )
                else:
                    self._backend_obj = self.backend.get_backend_session_obj().get_fx_chain_audio_input_port(
                        maybe_fx_chain.get_backend_obj(),
                        idx
                    )
                self.push_state()

    def maybe_initialize_external(self, name_hint, direction):
        if self._backend_obj:
            return # never create_backend more than once
        self._backend_obj = self.backend.open_audio_port(name_hint, direction)
        self.push_state()

    def push_state(self):
        self.set_muted(self.muted)
        self.set_passthrough_muted(self.passthrough_muted)
        self.set_gain(self.gain)
        
    def maybe_initialize_impl(self, name_hint, direction, is_internal):
        if is_internal:
            self.maybe_initialize_internal(name_hint, direction)
        else:
            self.maybe_initialize_external(name_hint, direction)
