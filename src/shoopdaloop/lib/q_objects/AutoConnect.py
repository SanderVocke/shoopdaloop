from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from .ShoopPyObject import *
from .FindParentBackend import FindParentBackend
from shoopdaloop.lib.backend_wrappers import *

from ..logging import Logger as BaseLogger

class AutoConnect(FindParentBackend):
    def __init__(self, parent=None):
        super(AutoConnect, self).__init__(parent)
        self.logger = BaseLogger("Frontend.AutoConnect")
        self._from_regex = None
        self._to_regex = None

        self.backendChanged.connect(lambda: self.update())
        self.backendInitializedChanged.connect(lambda: self.update())
    
    ######################
    ## SIGNALS
    ######################
    connected = ShoopSignal()

    ######################
    # PROPERTIES
    ######################
    
    # internal_port
    internalPortChanged = ShoopSignal('QVariant')
    @ShoopProperty(str, notify=internalPortChanged)
    def internal_port(self):
        return self._internal_port
    @internal_port.setter
    def internal_port(self, l):
        if l and l != self._internal_port:
            self._internal_port = l
            self.internalPortChanged.emit(l)
            self.update()
    
    # to_regex
    toRegexChanged = ShoopSignal('QVariant')
    @ShoopProperty(str, notify=toRegexChanged)
    def to_regex(self):
        return self._to_regex
    @to_regex.setter
    def to_regex(self, l):
        if l and l != self._to_regex:
            self._to_regex = l
            self.toRegexChanged.emit(l)
            self.update()
    
    ######################
    ## SLOTS
    ######################
    
    @ShoopSlot()
    def destroy(self):
        self._from_regex = None
        self._to_regex = None
        self.update()
    
    @ShoopSlot()
    def update(self):
        if self._backend and self._internal_port and self._backend.initialized:
            external_candidates = None
            my_connections = self._internal_port.get_connections_state()

            if self._to_regex is not None:
                external_candidates = self._backend.find_external_ports(self._to_regex, None, PortDirection.Input)
            
            self.logger.trace(lambda: f"AutoConnect update: internal_port={self._internal_port}, external_candidates={external_candidates}")

            for c in external_candidates:
                if c.direction != self._internal_port.direction and c.name not in my_connections.keys():
                    self.logger.info(lambda: f"Autoconnecting {self._internal_port.name} to {c.name}")
                    self._internal_port.connect_external_port(c.name)
                    self.connected.emit()