from typing import *

from .ShoopPyObject import *
from PySide6.QtQuick import QQuickItem
from PySide6.QtCore import SIGNAL, SLOT, QObject

from .Backend import Backend
from ..logging import Logger

from ..findFirstParent import findFirstParent

# Wraps a back-end port.
class FindParentBackend(ShoopQQuickItem):
    def __init__(self, parent=None):
        super(FindParentBackend, self).__init__(parent)
        self._backend = None
        self.logger = Logger("Frontend.FindParentBackend")
        
        self.rescan_parents()
        if not self._backend:
            self.parentChanged.connect(lambda: self.rescanBecauseParentChanged())
        
        self.logger.debug(lambda: f"{self.metaObject().className()}: Created, parent is {self.parent()}")

    ######################
    # PROPERTIES
    ######################

    # backend
    backendChanged = ShoopSignal(Backend)
    @ShoopProperty(Backend, notify=backendChanged)
    def backend(self):
        return self._backend
    @backend.setter
    def backend(self, l):
        if l and l != self._backend:
            self._backend = l
            self.logger.trace(lambda: f'{self.metaObject().className()}: Backend -> {l}')
            QObject.connect(self._backend, SIGNAL("readyChanged()"), self, SLOT("update()")) 
            self.backendChanged.emit(l)
            self.update()
    
    # backend_ready
    backendReadyChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=backendReadyChanged)
    def backend_ready(self):
        return self._backend and self._backend.property("ready")
    
    ###########
    ## SLOTS
    ###########

    def close(self):
        if self._backend:
            self.backend = None
            
    @ShoopSlot()
    def rescanBecauseParentChanged(self):
        self.logger.debug(lambda: f"{self.metaObject().className()}: rescanBecauseParentChanged")
        self.rescan_parents()
    
    @ShoopSlot()
    def update(self):
        value = self.backend_ready
        self.backendReadyChanged.emit(value)
            
    @ShoopSlot()
    def rescan_parents(self):
        maybe_backend = findFirstParent(self, lambda p: p and p.objectName() == "shoop_backend_wrapper" and self._backend == None)
        self.logger.debug(lambda: f"{self.metaObject().className()}: rescan_parents found {maybe_backend}")
        if maybe_backend:
            self.backend = maybe_backend