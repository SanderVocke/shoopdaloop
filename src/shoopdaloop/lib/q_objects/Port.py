
import re
import time
import os
import tempfile
import json
from typing import *
import sys

from .FindParentBackend import FindParentBackend
from .ShoopPyObject import *

from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer, Qt
from PySide6.QtQuick import QQuickItem

from .Backend import Backend
from ..logging import Logger

from ..findFirstParent import findFirstParent

# Wraps a back-end port.
class Port(FindParentBackend):
    def __init__(self, parent=None):
        super(Port, self).__init__(parent)
        self._name_hint = None
        self._backend_obj = None
        self._input_connectability = None
        self._output_connectability = None
        self._initialized = False
        self._internal_port_connections = []
        self._internally_connected_to = []
        self._name = self._new_name = ''
        self._muted = self._new_muted = None
        self._passthrough_muted = self._new_passthrough_muted = None
        self._is_internal = None
        self._ever_initialized = False
        self._n_ringbuffer_samples = None
        self.logger = Logger("Frontend.Port")
        
        self.backendChanged.connect(lambda: self.maybe_initialize())
        self.backendInitializedChanged.connect(lambda: self.maybe_initialize())
        self.initializedChanged.connect(lambda: self.update_internal_connections())

    ######################
    # PROPERTIES
    ######################

    # initialized
    initializedChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=initializedChanged)
    def initialized(self):
        return self._initialized

    # name hint
    nameHintChanged = ShoopSignal(str)
    @ShoopProperty(str, notify=nameHintChanged)
    def name_hint(self):
        return self._name_hint if self._name_hint != None else ''
    @name_hint.setter
    def name_hint(self, n):
        if n != self._name_hint and n != '':
            if self._name_hint != None:
                raise Exception('Port name hint may only be set once.')
            self.logger.debug(lambda: f'name_hint -> {n}')
            self._name_hint = n
            self.nameHintChanged.emit(n)
            self.maybe_initialize()
    
    # input connectability
    inputConnectabilityChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=inputConnectabilityChanged)
    def input_connectability(self):
        return self._input_connectability
    @input_connectability.setter
    def input_connectability(self, d):
        if d != self._input_connectability:
            if self._input_connectability != None:
                raise Exception('Port input connectability may only be set once.')
            self._input_connectability = d
            self.maybe_initialize()
    
    # output connectability
    outputConnectabilityChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=outputConnectabilityChanged)
    def output_connectability(self):
        return self._output_connectability
    @output_connectability.setter
    def output_connectability(self, d):
        if d != self._output_connectability:
            if self._output_connectability != None:
                raise Exception('Port output connectability may only be set once.')
            self._output_connectability = d
            self.maybe_initialize()
    
    # is_internal
    isInternalChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=isInternalChanged)
    def is_internal(self):
        return (self._is_internal if self._is_internal != None else False)
    @is_internal.setter
    def is_internal(self, d):
        if d != self._is_internal:
            if self._is_internal != None:
                raise Exception('Port internal-ness may only be set once.')
            self._is_internal = d
            self.maybe_initialize()
    
    # name
    nameChanged = ShoopSignal(str)
    @ShoopProperty(str, notify=nameChanged)
    def name(self):
        return self._name
    @name.setter
    def name(self, s):
        if self._name != s:
            self.logger.debug(lambda: f'name -> {s}')
            self._name = s
            self.nameChanged.emit(s)

    # muted
    mutedChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=mutedChanged)
    def muted(self):
        return self._muted if self._muted != None else False
    @muted.setter
    def muted(self, s):
        if self._muted != s:
            self.logger.debug(lambda: f'muted -> {s}')
            self._muted = s
            self.mutedChanged.emit(s)
            self.maybe_initialize()
    
    # passthrough_muted
    passthroughMutedChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=passthroughMutedChanged)
    def passthrough_muted(self):
        return self._passthrough_muted if self._passthrough_muted != None else False
    @passthrough_muted.setter
    def passthrough_muted(self, s):
        if self._passthrough_muted != s:
            self.logger.debug(lambda: f'passthrough muted -> {s}')
            self._passthrough_muted = s
            self.maybe_initialize()
            self.passthroughMutedChanged.emit(s)
    
    # internal_port_connections : ports to which to connect internally
    internalPortConnectionsChanged = ShoopSignal(list)
    @ShoopProperty(list, notify=internalPortConnectionsChanged)
    def internal_port_connections(self):
        return self._internal_port_connections
    @internal_port_connections.setter
    def internal_port_connections(self, s):
        if self._internal_port_connections != s:
            self._internal_port_connections = s
            self.update_internal_connections()
            self.maybe_initialize()
            self.internalPortConnectionsChanged.emit(s)

    # n_ringbuffer_samples
    nRingbufferSamplesChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=nRingbufferSamplesChanged)
    def n_ringbuffer_samples(self):
        return self._n_ringbuffer_samples
    @n_ringbuffer_samples.setter
    def n_ringbuffer_samples(self, n):
        if self._n_ringbuffer_samples != n:
            self.logger.debug(lambda: f'n ringbuffer samples -> {n}')
            self._n_ringbuffer_samples = n
            self.nRingbufferSamplesChanged.emit(n)
            self.maybe_initialize()
    
    ###########
    ## SLOTS
    ###########

    # Update from the back-end.
    @ShoopSlot(thread_protection = ThreadProtectionType.OtherThread)
    def updateOnOtherThread(self):
        raise Exception('Unimplemented in base class')
    
    # Update GUI thread.
    @ShoopSlot()
    def updateOnGuiThread(self):
        raise Exception('Unimplemented in base class')
    
    # Get the wrapped back-end object.
    @ShoopSlot(result='QVariant')
    def get_backend_obj(self):
        return self._backend_obj
    
    @ShoopSlot()
    def close(self):
        FindParentBackend.close(self)
        if self._backend_obj:
            self.logger.debug(lambda: "{}: Closing port {}".format(self, self._name))
            self._backend_obj.destroy()
            self._backend_obj = None
            self._initialized = False
            self.initializedChanged.emit(False)
    
    @ShoopSlot(bool)
    def set_muted(self, muted):
        if self._backend_obj:
            self._backend_obj.set_muted(muted)
        else:
            self.muted = muted
            self.maybe_initialize()

    @ShoopSlot(int)
    def set_min_n_ringbuffer_samples(self, n):
        if self._backend_obj:
            self._backend_obj.set_ringbuffer_n_samples(n)
        else:
            self.n_ringbuffer_samples = n
            self.maybe_initialize()
    
    @ShoopSlot(bool)
    def set_passthrough_muted(self, muted):
        if self._backend_obj:
            self._backend_obj.set_passthrough_muted(muted)
        else:
            self.passthrough_muted = muted
            self.maybe_initialize()
    
    @ShoopSlot()
    def maybe_initialize(self):
        self.logger.trace(lambda: 'maybe_initialize {}'.format(self._name_hint))
        if (not self._backend_obj) and \
            (not self._ever_initialized) and \
            self._name_hint != None and \
            self._input_connectability != None and \
            self._output_connectability != None and \
            self._is_internal != None and \
            self._muted != None and \
            self._passthrough_muted != None and \
            self._n_ringbuffer_samples != None and \
            self._backend and \
            self._backend.initialized:
            
            self.logger.trace(lambda: "{}: Initializing port {}".format(self, self._name_hint))
            self.maybe_initialize_impl(self._name_hint, self._input_connectability, self._output_connectability, self._is_internal)
            if self._backend_obj:
                self.logger.debug(lambda: "Initialized port {}".format(self._name_hint))
                self._initialized = True
                self.initializedChanged.emit(True)
                self._ever_initialized = True

    @ShoopSlot(str)
    def connect_external_port(self, name):
        if self._backend_obj:
            self._backend_obj.connect_external_port(name)
        else:
            self.logger.warning("Attempted to connect uninitialized port {}".format(self._name_hint))
    
    @ShoopSlot(str)
    def disconnect_external_port(self, name):
        if self._backend_obj:
            self._backend_obj.disconnect_external_port(name)
        else:
            self.logger.warning("Attempted to disconnect uninitialized port {}".format(self._name_hint))

    @ShoopSlot(result='QVariant')
    def get_connections_state(self):
        if self._backend_obj:
            return (self._backend_obj.get_connections_state() if self._backend_obj else dict())
        else:
            return dict()

    @ShoopSlot(result=list)
    def get_connected_external_ports(self):
        state = self.get_connections_state()
        return [k for k in state.keys() if state[k]]
    
    @ShoopSlot(list)
    def try_make_connections(self, port_names):
        if not self.initialized:
            return
        connected = self.get_connected_external_ports()
        for p in connected:
            if not p in port_names:
                self.disconnect_external_port(p)
        for p in port_names:
            if not p in connected:
                self.connect_external_port(p)

    ##########
    ## INTERNAL MEMBERS
    ##########
    def maybe_initialize_impl(self, name_hint, input_connectability, output_connectability, is_internal):
        raise Exception('Unimplemented in base class')
    
    def update_internal_connections(self):
        for other in self._internal_port_connections:
            if other and other.initialized and self.initialized and other not in self._internally_connected_to:
                self._backend_obj.connect_internal(other.get_backend_obj())
            elif other and not other.initialized:
                other.initializedChanged.connect(lambda: self.update_internal_connections())
