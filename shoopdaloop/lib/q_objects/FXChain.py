import re
import time
import os
import tempfile
import json
from typing import *
import sys

from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from ..backend_wrappers import *
from ..mode_helpers import is_playing_mode
from ..q_objects.Backend import Backend
from ..findFirstParent import findFirstParent
from ..findChildItems import findChildItems

# Wraps a back-end FX chain.
class FXChain(QQuickItem):
    def __init__(self, parent=None):
        super(FXChain, self).__init__(parent)
        self._running = False
        self._ui_visible = False
        self._initialized = False
        self._backend = None
        self._backend_object = None
        self._chain_type = None

        self.rescan_parents()
        if not self._backend:
            self.parentChanged.connect(self.rescan_parents)

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
            if self._backend or self._backend_object:
                raise Exception('May not change backend of existing FX chain')
            self._backend = l
            self.maybe_initialize()
    
    # initialized
    initializedChanged = Signal(bool)
    @Property(bool, notify=initializedChanged)
    def initialized(self):
        return self._initialized

    # ui_visible
    uiVisibleChanged = Signal(bool)
    @Property(bool, notify=uiVisibleChanged)
    def ui_visible(self):
        return self._ui_visible
    # Indirect setter via back-end
    @Slot(bool)
    def set_ui_visible(self, value):
        if self._backend_object:
            self._backend_object.set_visible(value)
        else:
            self.initializedChanged.connect(lambda: self._backend_object.set_visible(value))
    
    # running
    runningChanged = Signal(bool)
    @Property(bool, notify=runningChanged)
    def running(self):
        return self._running
    
    # chain type
    chainTypeChanged = Signal(int)
    @Property(int, notify=chainTypeChanged)
    def chain_type(self):
        return (self._chain_type if self._chain_type != None else -1)
    @chain_type.setter
    def chain_type(self, l):
        if l != None and l != self._chain_type:
            if self._chain_type:
                raise Exception('May not change chain type of existing FX chain')
            self._chain_type = l
            if not self._backend:
                self.rescan_parents()
            else:
                self.maybe_initialize()
    
    ###########
    ## SLOTS
    ###########

    # Update mode from the back-end.
    @Slot()
    def update(self):
        if not self.initialized:
            return
        
        prev_running = self._running
        prev_ui_visible = self._ui_visible

        state = self._backend_object.get_state()
        self._running = state.running
        self._ui_visible = state.visible

        if prev_ui_visible != self._ui_visible:
            self.uiVisibleChanged.emit(self._ui_visible)
        if prev_running != self._running:
            self.runningChanged.emit(self._running)
    
    @Slot()
    def rescan_parents(self):
        maybe_backend = findFirstParent(self, lambda p: p and isinstance(p, QQuickItem) and p.inherits('Backend') and self._backend == None)
        if maybe_backend:
            self.backend = maybe_backend
        print("FX chain: rescan parents, tp {}, result {}".format(self._chain_type, self._backend))
    
    @Slot(result='QVariant')
    def get_backend(self):
        maybe_backend = findFirstParent(self, lambda p: p and isinstance(p, QQuickItem) and p.inherits('Backend'))
        if maybe_backend:
            return maybe_backend
        raise Exception("Could not find backend!")
    
    @Slot(result='QVariant')
    def get_backend_obj(self):
        return self._backend_object
    
    @Slot(result="QVariant")
    def get_internal_state(self):
        if self._backend_object:
            rval = self._backend_object.get_internal_state()
            print("Got internal state: {}".format(rval))
            return rval
        raise Exception("Getting internal state of uninitialized FX chain")
    
    @Slot(str)
    def restore_internal_state(self, state):
        if self._backend_object:
            print("Restoring internal state: {}".format(state))
            self._backend_object.restore_internal_state(state)
        else:
            raise Exception("Restoring internal state of uninitialized FX chain")
    
    def maybe_initialize(self):
        if self._backend and self._backend.initialized and self._chain_type != None and not self._backend_object:
            self._backend_object = self._backend.get_backend_obj().create_fx_chain(FXChainType(self._chain_type))
            if self._backend_object:
                self._initialized = True
                self.update()
                self._backend.registerBackendObject(self)
                self.initializedChanged.emit(True)