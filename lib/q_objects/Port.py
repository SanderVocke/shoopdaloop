from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
from PyQt6.QtQuick import QQuickItem
import re
import time
import os
import tempfile
import json
from typing import *

import sys
sys.path.append('../..')

import lib.backend_wrappers as backend
from lib.q_objects.Backend import Backend
from lib.findFirstParent import findFirstParent

# Wraps a back-end port.
class Port(QQuickItem):
    def __init__(self, parent=None):
        super(Port, self).__init__(parent)
        self._name_hint = ''
        self._backend_obj = None
        self._direction = ''
        self._name = ''
        self._initialized = False
        self._backend = None

        self.rescan_parents()
        if not self._backend:
            self.parentChanged.connect(self.rescan_parents)

    ######################
    # PROPERTIES
    ######################

    # backend
    backendChanged = pyqtSignal(Backend)
    @pyqtProperty(Backend, notify=backendChanged)
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
    initializedChanged = pyqtSignal(bool)
    @pyqtProperty(bool, notify=initializedChanged)
    def initialized(self):
        return self._initialized

    # name hint
    nameHintChanged = pyqtSignal(str)
    @pyqtProperty(str, notify=nameHintChanged)
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
    directionChanged = pyqtSignal(str)
    @pyqtProperty(str, notify=directionChanged)
    def direction(self):
        return self._direction
    @direction.setter
    def direction(self, d):
        if d != self._direction:
            if self._direction != '':
                raise Exception('Port direction may only be set once.')
            if d not in ['input', 'output']:
                raise Exception('Port direction may only be "input" or "output"')
            self._direction = d
            self.maybe_initialize()
    
    # name
    nameChanged = pyqtSignal(str)
    @pyqtProperty(str, notify=nameChanged)
    def name(self):
        return self._name
    @name.setter
    def volume(self, s):
        if self._name != s:
            self._name = s
            self.nameChanged.emit(s)
    
    ###########
    ## SLOTS
    ###########

    # Update mode from the back-end.
    @pyqtSlot()
    def update(self):
        raise Exception('Unimplemented in base class')
    
    # Get the wrapped back-end object.
    @pyqtSlot(result='QVariant')
    def get_backend_obj(self):
        return self._backend_obj
    
    @pyqtSlot()
    def close(self):
        if self._backend_obj:
            self._backend_obj.destroy()
            self._backend_obj = None
    
    @pyqtSlot()
    def rescan_parents(self):
        maybe_backend = findFirstParent(self, lambda p: p and isinstance(p, QObject) and p.inherits('Backend') and self._backend == None)
        if maybe_backend:
            self.backend = maybe_backend

    ##########
    ## INTERNAL MEMBERS
    ##########
    def maybe_initialize_impl():
        raise Exception('Unimplemented in base class')

    def maybe_initialize(self):
        if self._name_hint != '' and self._direction != '' and self._backend and not self._backend_obj:
            direction = None
            if self._direction == 'input':
                direction = backend.PortDirection.Input
            elif self._direction == 'output':
                direction = backend.PortDirection.Output
            if not direction:
                raise Exception("Invalid direction.")
            self.maybe_initialize_impl(self._name_hint, direction)
            if self._backend_obj:
                self._initialized = True
                self.initializedChanged.emit(True)