import re
import time
import os
import tempfile
import json
from typing import *
import sys

from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer, Qt, Q_ARG, Q_RETURN_ARG, SIGNAL, SLOT
from PySide6.QtQuick import QQuickItem

from .ShoopPyObject import *
from .Port import Port

import shoop_py_backend
from ..findFirstParent import findFirstParent
from ..findChildItems import findChildItems
from ..logging import Logger

# Wraps a back-end port.
class AudioPort(Port):
    def __init__(self, parent=None):
        super(AudioPort, self).__init__(parent)
        self._input_peak = self._new_input_peak = 0.0
        self._output_peak = self._new_output_peak = 0.0
        self._gain = self._new_gain = 1.0
        self.logger = Logger("Frontend.AudioPort")
        self._n_updates_pending = 0
        self._signal_sender = ThreadUnsafeSignalEmitter()
        
        self._signal_sender.signal.connect(self.updateOnGuiThread, Qt.QueuedConnection)

    ######################
    # PROPERTIES
    ######################

    # input_peak
    inputPeakChanged = ShoopSignal(float)
    @ShoopProperty(float, notify=inputPeakChanged)
    def input_peak(self):
        return self._input_peak
    @input_peak.setter
    def input_peak(self, s):
        if self._input_peak != s:
            self._input_peak = s
            self.inputPeakChanged.emit(s)
    
    # output_peak
    outputPeakChanged = ShoopSignal(float)
    @ShoopProperty(float, notify=outputPeakChanged)
    def output_peak(self):
        return self._output_peak
    @output_peak.setter
    def output_peak(self, s):
        if self._output_peak != s:
            self._output_peak = s
            self.outputPeakChanged.emit(s)
    
    # gain
    gainChanged = ShoopSignal(float)
    @ShoopProperty(float, notify=gainChanged)
    def gain(self):
        return self._gain
    @gain.setter
    def gain(self, s):
        if self._gain != s:
            self.logger.debug(lambda: f'gain -> {s}')
            self._gain = s
            self.gainChanged.emit(s)
    
    ###########
    ## SLOTS
    ###########

    # Update from the back-end.
    @ShoopSlot(thread_protection = ThreadProtectionType.OtherThread)
    def updateOnOtherThread(self):
        self.logger.trace(lambda: f'update on back-end thread (initialized {self._initialized})')
        if not self._initialized:
            return
        state = self._backend_obj.get_state()
        self._new_input_peak = state.input_peak
        self._new_output_peak = state.output_peak
        self._new_name = state.name
        self._new_gain = state.gain
        self._new_muted = state.muted
        self._new_passthrough_muted = state.passthrough_muted
        self._n_ringbuffer_samples = state.ringbuffer_n_samples
        self._n_updates_pending += 1

        self._signal_sender.do_emit()
    
    # Update the GUI thread.
    @ShoopSlot()
    def updateOnGuiThread(self):
        self.logger.trace(lambda: f'update on GUI thread (# {self._n_updates_pending}, initialized {self._initialized})')
        if not self._initialized or not self.isValid():
            return
        if self._n_updates_pending == 0:
            return
        self.input_peak = self._new_input_peak
        self.output_peak = self._new_output_peak
        if self.name != self._new_name:
            self.logger.debug(lambda: f"name -> {self._new_name}")
            self.name = self._new_name
        if self.gain != self._new_gain:
            self.logger.debug(lambda: f"gain -> {self._new_gain}")
            self.gain = self._new_gain
        if self.muted != self._new_muted:
            self.logger.debug(lambda: f"muted -> {self._new_muted}")
            self.muted = self._new_muted
        if self.passthrough_muted != self._new_passthrough_muted:
            self.logger.debug(lambda: f"passthrough_muted -> {self._new_passthrough_muted}")
            self.passthrough_muted = self._new_passthrough_muted
        if self.n_ringbuffer_samples != self._n_ringbuffer_samples:
            self.logger.debug(lambda: f"n_ringbuffer_samples -> {self._n_ringbuffer_samples}")
            self.n_ringbuffer_samples = self._n_ringbuffer_samples
        self._n_updates_pending = 0
    
    @ShoopSlot(float)
    def set_gain(self, gain):
        if self._backend_obj:
            self._backend_obj.set_gain(gain)
    
    @ShoopSlot(list)
    def dummy_queue_data(self, data):
        self._backend_obj.dummy_queue_data(data)
    
    @ShoopSlot(int, result=list)
    def dummy_dequeue_data(self, n):
        return self._backend_obj.dummy_dequeue_data(n)
    
    @ShoopSlot(int)
    def dummy_request_data(self, n):
        self._backend_obj.dummy_request_data(n)

    ##########
    ## INTERNAL MEMBERS
    ##########
    
    def get_data_type(self):
        return int(shoop_py_backend.PortDataType.Audio)
    
    def maybe_initialize_internal(self, name_hint, input_connectability, output_connectability):
        # Internal ports are owned by FX chains.
        maybe_fx_chain = findFirstParent(self, lambda p: p and isinstance(p, QQuickItem) and p.inherits('FXChain'))
        if maybe_fx_chain and not self._backend_obj:
            if not maybe_fx_chain.initialized:
                maybe_fx_chain.initializedChanged.connect(lambda: self.maybe_initialize())
            else:
                self.logger.debug("Initialize internal")
                # Determine our index in the FX chain
                def find_index():
                    idx = 0
                    for port in findChildItems(maybe_fx_chain, lambda i: i.inherits('AudioPort')):
                        if port == self:
                            return idx
                        elif port.input_connectability == self.input_connectability and port.output_connectability == self.output_connectability:
                            idx += 1
                    return None
                idx = find_index()
                if idx == None:
                    self.logger.throw_error('Could not find self in FX chain')
                # Now request our backend object.
                n_ringbuffer = self.n_ringbuffer_samples
                if not (self.output_connectability & int(shoop_py_backend.PortConnectabilityKind.Internal)):
                    self._backend_obj = maybe_fx_chain.get_backend_obj().get_audio_input_port(idx)
                else:
                    self._backend_obj = maybe_fx_chain.get_backend_obj().get_audio_output_port(idx)
                self.push_state()
                self.set_min_n_ringbuffer_samples (n_ringbuffer)
                self.connect_backend_updates()

    def maybe_initialize_external(self, name_hint, input_connectability, output_connectability):
        if self._backend_obj:
            return # never create_backend more than once
        self.logger.debug(lambda: "Initialize external")
        from shoop_rust import shoop_rust_open_driver_audio_port
        from shiboken6 import getCppPointer
        direction = int(shoop_py_backend.PortDirection.Input) if not (input_connectability & int(shoop_py_backend.PortConnectabilityKind.Internal)) else int(shoop_py_backend.PortDirection.Output)
        self._backend_obj = shoop_rust_open_driver_audio_port(
            getCppPointer(self._backend)[0],
            name_hint,
            direction,
            self.n_ringbuffer_samples
        )
        self.logger.trace(lambda: f'backend_obj = {self._backend_obj}')
        self.push_state()
        self.connect_backend_updates()
    
    def connect_backend_updates(self):
        QObject.connect(self._backend, SIGNAL("updated_on_gui_thread()"), self, SLOT("updateOnGuiThread()"), Qt.DirectConnection)
        QObject.connect(self._backend, SIGNAL("updated_on_backend_thread()"), self, SLOT("updateOnOtherThread()"), Qt.DirectConnection)

    def push_state(self):
        self.set_muted(self.muted)
        self.set_passthrough_muted(self.passthrough_muted)
        self.set_gain(self.gain)
        
    def maybe_initialize_impl(self, name_hint, input_connectability, output_connectability, is_internal):
        if is_internal:
            self.maybe_initialize_internal(name_hint, input_connectability, output_connectability)
        else:
            self.maybe_initialize_external(name_hint, input_connectability, output_connectability)
