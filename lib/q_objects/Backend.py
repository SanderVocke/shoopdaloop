from PyQt6.QtCore import Qt, QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
from PyQt6.QtQuick import QQuickItem
import re
import time
import os
import tempfile
import json

import sys
sys.path.append('../..')

import lib.backend_wrappers as backend
from lib.findChildItems import findChildItems

# Wraps the back-end.
class Backend(QQuickItem):
    def __init__(self, parent=None):
        super(Backend, self).__init__(parent)
        self._update_interval_ms = 0
        self._timer = None
        self._initialized = False
        self._client_name_hint = None
        self._backend_type = None
        self._backend_obj = None
        self.destroyed.connect(self.close)
    
    update = pyqtSignal()

    ######################
    # PROPERTIES
    ######################

    updateIntervalMsChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=updateIntervalMsChanged)
    def update_interval_ms(self):
        return self._update_interval_ms
    @update_interval_ms.setter
    def update_interval_ms(self, u):
        if u != self._update_interval_ms:
            self._update_interval_ms = u
            if self._timer:
                self._timer.destroy()
                self._timer = None
            self._timer = QTimer(self)
            self._timer.setSingleShot(False)
            self._timer.timeout.connect(self.doUpdate)
            self._timer.start(self._update_interval_ms)
    
    initializedChanged = pyqtSignal(bool)
    @pyqtProperty(bool, notify=initializedChanged)
    def initialized(self):
        return self._initialized
    
    clientNameHintChanged = pyqtSignal(str)
    @pyqtProperty(str, notify=clientNameHintChanged)
    def client_name_hint(self):
        return (self._client_name_hint if self._client_name_hint else "")
    @client_name_hint.setter
    def client_name_hint(self, n):
        if self._initialized:
            raise Exception("Back-end client name hint may only be set once.")
        self._client_name_hint = n
        if self._client_name_hint != None and self._backend_type != None:
            self.init()

    backendTypeChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=backendTypeChanged)
    def backend_type(self):
        return (self._backend_type.value if self._backend_type else backend.BackendType.Dummy)
    @backend_type.setter
    def backend_type(self, n):
        if self._initialized:
            raise Exception("Back-end type can only be set once.")
        self._backend_type = backend.BackendType(n)
        if self._backend_type != None and self._client_name_hint != None:
            self.init()
    
    ###########
    ## SLOTS
    ###########

    @pyqtSlot()
    def doUpdate(self):
        from lib.q_objects.Loop import Loop
        for loop in findChildItems(self, lambda c: isinstance(c, Loop)):
            loop.update()
        self.update.emit()
    
    @pyqtSlot(result=int)
    def get_sample_rate(self):
        return self._backend_obj.get_sample_rate()

    @pyqtSlot()
    def close(self):
        if self._initialized:
            self._backend_obj.terminate()
        self._initialized = False
    
    # Get the wrapped back-end object.
    @pyqtSlot(result='QVariant')
    def get_backend_obj(self):
        return self._backend_obj
    
    ################
    ## INTERNAL METHODS
    ################

    def init(self):
        if self._initialized:
            raise Exception("May not initialize more than one back-end at a time.")
        self._backend_obj = backend.init_backend(self._backend_type, self._client_name_hint)
        self._initialized = True
        