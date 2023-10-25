
import re
import time
import os
import tempfile
import json
from typing import *
import sys

from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from .Backend import Backend
from ..logging import Logger

from ..findFirstParent import findFirstParent

# Wraps a back-end port.
class Port(QQuickItem):
    def __init__(self, parent=None):
        super(Port, self).__init__(parent)
        self._name_hint = None
        self._backend_obj = None
        self._direction = None
        self._initialized = False
        self._backend = None
        self._passthrough_to = []
        self._passthrough_connected_to = []
        self._name = ''
        self._muted = None
        self._passthrough_muted = None
        self._is_internal = None
        self._ever_initialized = False
        self.__logger = Logger("Frontend.Port")
        
        self.rescan_parents()
        if not self._backend:
            self.parentChanged.connect(lambda: self.rescan_parents())
        
        self.initializedChanged.connect(lambda: self.update_passthrough_connections())

    ######################
    # PROPERTIES
    ######################

    # backend
    backendChanged = Signal(Backend)
    @Property(Backend, notify=backendChanged)
    def backend(self):
        return self._backend
    @backend.setter
    def backend(self, l):
        if l and l != self._backend:
            if self._backend or self._backend_obj:
                raise Exception('May not change backend of existing port')
            self._backend = l
            self._backend.initializedChanged.connect(lambda: self.maybe_initialize())
            self.maybe_initialize()

    # initialized
    initializedChanged = Signal(bool)
    @Property(bool, notify=initializedChanged)
    def initialized(self):
        return self._initialized

    # name hint
    nameHintChanged = Signal(str)
    @Property(str, notify=nameHintChanged)
    def name_hint(self):
        return self._name_hint if self._name_hint != None else ''
    @name_hint.setter
    def name_hint(self, n):
        if n != self._name_hint:
            if self._name_hint != None:
                raise Exception('Port name hint may only be set once.')
            self._name_hint = n
            self.maybe_initialize()
    
    # direction
    directionChanged = Signal(int)
    @Property(int, notify=directionChanged)
    def direction(self):
        return self._direction
    @direction.setter
    def direction(self, d):
        if d != self._direction:
            if self._direction != None:
                raise Exception('Port direction may only be set once.')
            self._direction = d
            self.maybe_initialize()
    
    # is_internal
    isInternalChanged = Signal(bool)
    @Property(bool, notify=isInternalChanged)
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
    nameChanged = Signal(str)
    @Property(str, notify=nameChanged)
    def name(self):
        return self._name
    @name.setter
    def name(self, s):
        if self._name != s:
            self._name = s
            self.nameChanged.emit(s)

    # muted
    mutedChanged = Signal(bool)
    @Property(bool, notify=mutedChanged)
    def muted(self):
        return self._muted if self._muted != None else False
    @muted.setter
    def muted(self, s):
        if self._muted != s:
            self._muted = s
            self.mutedChanged.emit(s)
    
    # passthrough_muted
    passthroughMutedChanged = Signal(bool)
    @Property(bool, notify=passthroughMutedChanged)
    def passthrough_muted(self):
        return self._passthrough_muted if self._passthrough_muted != None else False
    @passthrough_muted.setter
    def passthrough_muted(self, s):
        if self._passthrough_muted != s:
            self._passthrough_muted = s
            self.maybe_initialize()
            self.passthroughMutedChanged.emit(s)
    
    # passthrough_to : ports to which to passthrough
    passthroughToChanged = Signal(list)
    @Property(list, notify=passthroughToChanged)
    def passthrough_to(self):
        return self._passthrough_to
    @passthrough_to.setter
    def passthrough_to(self, s):
        if self._passthrough_to != s:
            self._passthrough_to = s
            self.update_passthrough_connections()
            self.maybe_initialize()
            self.passthroughToChanged.emit(s)
    
    ###########
    ## SLOTS
    ###########

    # Update mode from the back-end.
    @Slot()
    def update(self):
        raise Exception('Unimplemented in base class')
    
    # Get the wrapped back-end object.
    @Slot(result='QVariant')
    def get_backend_obj(self):
        return self._backend_obj
    
    @Slot()
    def close(self):
        if self._backend_obj:
            self.__logger.debug(lambda: "{}: Closing port {}".format(self, self._name))
            self._backend_obj.destroy()
            self._backend_obj = None
            self._initialized = False
            self.initializedChanged.emit(False)
    
    @Slot()
    def rescan_parents(self):
        maybe_backend = findFirstParent(self, lambda p: p and isinstance(p, QQuickItem) and p.inherits('Backend') and self._backend == None)
        if maybe_backend:
            self.backend = maybe_backend
    
    @Slot(bool)
    def set_muted(self, muted):
        if self._backend_obj:
            self._backend_obj.set_muted(muted)
    
    @Slot(bool)
    def set_passthrough_muted(self, muted):
        if self._backend_obj:
            self._backend_obj.set_passthrough_muted(muted)
    
    @Slot()
    def maybe_initialize(self):
        if (not self._backend_obj) and \
            (not self._ever_initialized) and \
            self._name_hint != None and \
            self._direction != None and \
            self._is_internal != None and \
            self._muted != None and \
            self._passthrough_muted != None and \
            self._backend and \
            self._backend.initialized:
            
            self.__logger.debug(lambda: "{}: Initializing port {}".format(self, self._name_hint))
            
            self.maybe_initialize_impl(self._name_hint, self._direction, self._is_internal)
            if self._backend_obj:
                self._initialized = True
                self._backend.registerBackendObject(self)
                self.initializedChanged.emit(True)
                self._ever_initialized = True

    @Slot(str)
    def connect_external_port(self, name):
        self._backend_obj.connect_external_port(name)
    
    @Slot(str)
    def disconnect_external_port(self, name):
        self._backend_obj.disconnect_external_port(name)

    @Slot(result='QVariant')
    def get_connections_state(self):
        return (self._backend_obj.get_connections_state() if self._backend_obj else dict())

    @Slot(result=list)
    def get_connected_external_ports(self):
        state = self.get_connections_state()
        return [k for k in state.keys() if state[k]]
    
    @Slot(list)
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
    def maybe_initialize_impl(self, name_hint, direction, is_internal):
        raise Exception('Unimplemented in base class')
    
    def update_passthrough_connections(self):
        for other in self._passthrough_to:
            if other and other.initialized and self.initialized and other not in self._passthrough_connected_to:
                self._backend_obj.connect_passthrough(other.get_backend_obj())
            elif other and not other.initialized:
                other.initializedChanged.connect(lambda: self.update_passthrough_connections())
