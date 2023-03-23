import re
import time
import os
import tempfile
import json
import sys

from PySide6.QtCore import Qt, QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from ..backend_wrappers import *
from ..findChildItems import findChildItems

# Wraps the back-end.
class Backend(QQuickItem):
    def __init__(self, parent=None):
        super(Backend, self).__init__(parent)
        self._update_interval_ms = 50
        self._timer = None
        self._initialized = False
        self._client_name_hint = None
        self._backend_type = None
        self._backend_obj = None
        self.destroyed.connect(self.close)
    
    update = Signal()

    ######################
    # PROPERTIES
    ######################

    updateIntervalMsChanged = Signal(int)
    @Property(int, notify=updateIntervalMsChanged)
    def update_interval_ms(self):
        return self._update_interval_ms
    @update_interval_ms.setter
    def update_interval_ms(self, u):
        if u != self._update_interval_ms:
            self._update_interval_ms = u
            self.init_timer()
    
    initializedChanged = Signal(bool)
    @Property(bool, notify=initializedChanged)
    def initialized(self):
        return self._initialized
    
    clientNameHintChanged = Signal(str)
    @Property(str, notify=clientNameHintChanged)
    def client_name_hint(self):
        return (self._client_name_hint if self._client_name_hint else "")
    @client_name_hint.setter
    def client_name_hint(self, n):
        if self._initialized:
            raise Exception("Back-end client name hint may only be set once.")
        self._client_name_hint = n
        self.maybe_init()

    backendTypeChanged = Signal(int)
    @Property(int, notify=backendTypeChanged)
    def backend_type(self):
        return (self._backend_type.value if self._backend_type else BackendType.Dummy)
    @backend_type.setter
    def backend_type(self, n):
        if self._initialized:
            raise Exception("Back-end type can only be set once.")
        self._backend_type = BackendType(n)
        self.maybe_init()
    
    ###########
    ## SLOTS
    ###########

    @Slot()
    def doUpdate(self):
        from lib.q_objects.Loop import Loop
        from lib.q_objects.Port import Port
        for loop in findChildItems(self, lambda c: isinstance(c, Loop)):
            loop.update()
        for port in findChildItems(self, lambda c: isinstance(c, Port)):
            port.update()
        self.update.emit()
    
    @Slot(result=int)
    def get_sample_rate(self):
        return self._backend_obj.get_sample_rate()

    @Slot()
    def close(self):
        if self._initialized:
            self._backend_obj.terminate()
        self._initialized = False
    
    # Get the wrapped back-end object.
    @Slot(result='QVariant')
    def get_backend_obj(self):
        return self._backend_obj
    
    @Slot()
    def maybe_init(self):
        if not self._initialized and self._client_name_hint != None and self._backend_type != None:
            self.init()
    
    ################
    ## INTERNAL METHODS
    ################

    def init(self):
        if self._initialized:
            raise Exception("May not initialize more than one back-end at a time.")
        self._backend_obj = init_backend(self._backend_type, self._client_name_hint)
        self._initialized = True
        self.init_timer()
    
    def init_timer(self):
        if self._timer:
            self._timer.active = False
            self._timer = None
        self._timer = QTimer(self)
        self._timer.setSingleShot(False)
        self._timer.timeout.connect(self.doUpdate)
        self._timer.start(self._update_interval_ms)
    