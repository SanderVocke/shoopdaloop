from typing import *

from .ShoopPyObject import *
from PySide6.QtQuick import QQuickItem

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
            self.parentChanged.connect(lambda: self.rescan_parents())

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
            self.logger.trace(lambda: f'Backend -> {l}')
            self._backend.initializedChanged.connect(lambda v: self.backendInitializedChanged.emit(v))
            self.backendChanged.emit(l)
    
    # backend_initialized
    backendInitializedChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=backendInitializedChanged)
    def backend_initialized(self):
        return self._backend.initialized
    
    ###########
    ## SLOTS
    ###########

    def close(self):
        if self._backend:
            self._backend.unregisterBackendObject(self)
            self.backend = None
    
    @ShoopSlot()
    def rescan_parents(self):
        maybe_backend = findFirstParent(self, lambda p: p and isinstance(p, QQuickItem) and p.inherits('Backend') and self._backend == None)
        if maybe_backend:
            self.backend = maybe_backend
            self.backend.registerBackendObject(self)