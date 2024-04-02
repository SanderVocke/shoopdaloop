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
        self._internal_port = None
        self._to_regex = None
        self._closed = False

        self.backendChanged.connect(self.update)
        self.backendInitializedChanged.connect(self.update)

        self._timer = QTimer(self)
        self._timer.setSingleShot(False)
        self._timer.setInterval(1000)
        self._timer.timeout.connect(self.update)
        self._timer.start()
    
    ######################
    ## SIGNALS
    ######################
    onlyExternalFound = ShoopSignal()
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
            self.logger.trace(lambda: f"internal_port -> {l}")
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
            self.logger.trace(lambda: f"to_regex -> {l}")
            self.toRegexChanged.emit(l)
            self.update()
    
    ######################
    ## SLOTS
    ######################
    
    @ShoopSlot()
    def destroy(self):
        self._timer.stop()
        self._to_regex = None
        self._internal_port = None
        self._backend = None
        self._closed = True
    
    @ShoopSlot()
    def update(self):
        if self._backend and self._internal_port and self._backend.initialized and not self._closed:
            internal_port = self._internal_port
            external_candidates = []
            my_connections = internal_port.get_connections_state() if internal_port else {}
            data_type = internal_port.get_data_type()

            if self._to_regex is not None:
                external_candidates = self._backend.find_external_ports(self._to_regex, PortDirection.Any.value, data_type)
            
            for c in external_candidates:
                connected = c.name in my_connections.keys() and my_connections[c.name] == True
                if connected:
                    self.logger.trace(lambda: f"{internal_port.name} already connected to {c.name}")
                elif c.direction != internal_port.direction:
                    if internal_port.initialized:
                        self.logger.info(lambda: f"Autoconnecting {internal_port.name} to {c.name}")
                        internal_port.connect_external_port(c.name)
                        self.connected.emit()
                        break
                    else:
                        self.logger.debug(lambda: f"Found external autoconnect port {c.name}, internal port not yet opened")
                        self.onlyExternalFound.emit()