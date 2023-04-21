
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

from ..findFirstParent import findFirstParent

# Wraps a back-end port.
class Port(QQuickItem):
    def __init__(self, parent=None):
        super(Port, self).__init__(parent)
        self._name_hint = ''
        self._backend_obj = None
        self._direction = None
        self._initialized = False
        self._backend = None
        self._passthrough_to = []
        self._passthrough_connected_to = []
        self._name = ''
        self._muted = False
        self._passthrough_muted = False
        
        self.rescan_parents()
        if not self._backend:
            self.parentChanged.connect(self.rescan_parents)
        
        self.initializedChanged.connect(self.update_passthrough_connections)

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
        return self._name_hint
    @name_hint.setter
    def name_hint(self, n):
        if n != self._name_hint:
            if self._name_hint != '':
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
    
    # name
    nameChanged = Signal(str)
    @Property(str, notify=nameChanged)
    def name(self):
        return self._name
    @name.setter
    def name(self, s):
        if self._name != s:
            self.nameChanged.emit(s)

    # muted
    mutedChanged = Signal(bool)
    @Property(bool, notify=mutedChanged)
    def muted(self):
        return self._muted
    @muted.setter
    def muted(self, s):
        if self._muted != s:
            self._muted = s
            self.mutedChanged.emit(s)
    
    # passthrough_muted
    passthroughMutedChanged = Signal(bool)
    @Property(bool, notify=passthroughMutedChanged)
    def passthrough_muted(self):
        return self._passthrough_muted
    @passthrough_muted.setter
    def passthrough_muted(self, s):
        if self._passthrough_muted != s:
            self._passthrough_muted = s
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

    ##########
    ## INTERNAL MEMBERS
    ##########
    def maybe_initialize_impl():
        raise Exception('Unimplemented in base class')

    def maybe_initialize(self):
        if self._name_hint != '' and self._direction != None and self._backend and self._backend.initialized and not self._backend_obj:
            self.maybe_initialize_impl(self._name_hint, self._direction)
            if self._backend_obj:
                self._initialized = True
                self._backend.registerBackendObject(self)
                self.initializedChanged.emit(True)
    
    def connect_passthrough_impl(self, other):
        raise Exception('Unimplemented in base class')
    
    def maybe_connect_passthrough(self, other):
        if other.initialized and self.initialized and other not in self._passthrough_connected_to:
            self.connect_passthrough_impl(other)
    
    def update_passthrough_connections(self):
        for other in self._passthrough_to:
            if other.initialized and self.initialized and other not in self._passthrough_connected_to:
                self.connect_passthrough_impl(other)
            else:
                other.initializedChanged.connect(lambda: self.maybe_connect_passthrough(other))
