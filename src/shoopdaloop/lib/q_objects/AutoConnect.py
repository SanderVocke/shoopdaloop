from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from ..logging import Logger as BaseLogger
from .JackControlClient import JackControlClient

import jacklib as jack

class AutoConnect(QQuickItem):
    def __init__(self, parent=None):
        super(AutoConnect, self).__init__(parent)
        self.logger = BaseLogger("Frontend.AutoConnect")
        self._from_regex = None
        self._to_regex = None
        
        self._jack = JackControlClient.get_instance()
        self._jack.portRegistered.connect(self.update)
        self._jack.portRenamed.connect(self.update)
        
        self._timer = QTimer()
        self._timer.setSingleShot(False)
        self._timer.setInterval(1000)
        self._timer.timeout.connect(self.update)
        self._timer.start()
    
    ######################
    ## SIGNALS
    ######################
    onlyExternalFound = Signal()
    connected = Signal()

    ######################
    # PROPERTIES
    ######################
    
    # from_regex
    fromRegexChanged = Signal('QVariant')
    @Property(str, notify=fromRegexChanged)
    def from_regex(self):
        return self._from_regex
    @from_regex.setter
    def from_regex(self, l):
        if l and l != self._from_regex:
            self._from_regex = l
            self.fromRegexChanged.emit(l)
            self.update()
    
    # to_regex
    toRegexChanged = Signal('QVariant')
    @Property(str, notify=toRegexChanged)
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
    
    @Slot()
    def destroy(self):
        self._from_regex = None
        self._to_regex = None
        self.update()
        self._jack = None
    
    @Slot()
    def update(self):
        from_candidates = None
        to_candidates = None
        any_external = False
        any_internal = False
        
        if self._from_regex is not None:
            from_candidates = self._jack.find_ports(self._from_regex, None, jack.JackPortIsOutput)
            any_external = any_external or (from_candidates and len(from_candidates) > 0 and (True not in [self._jack.port_is_mine(p) for p in from_candidates]))
            any_internal = any_internal or (from_candidates and len(from_candidates) > 0 and (True in [self._jack.port_is_mine(p) for p in from_candidates]))
        
        if self._to_regex is not None:
            to_candidates = self._jack.find_ports(self._to_regex, None, jack.JackPortIsInput)
            any_external = any_external or (to_candidates and len(to_candidates) > 0 and (True not in [self._jack.port_is_mine(p) for p in to_candidates]))
            any_internal = any_internal or (to_candidates and len(to_candidates) > 0 and (True in [self._jack.port_is_mine(p) for p in to_candidates]))
        
        self.logger.trace(lambda: "AutoConnect update: from_candidates={}, to_candidates={}, any_external={}, any_internal={}".format(from_candidates, to_candidates, any_external, any_internal))
        
        if any_external and not any_internal:
            self.onlyExternalFound.emit()
        
        if (from_candidates is None) or (to_candidates is None):
            return
        
        if len(from_candidates) == 0 and len(to_candidates) == 0:
            return
        
        if len(from_candidates) > 1 and len(to_candidates) > 1:
            self.logger.warning(lambda: "Multiple ports match both regexes, not autoconnecting")
            return

        for _from in from_candidates:
            for _to in to_candidates:
                if _to not in self._jack.all_port_connections(_from):
                    self.logger.info(lambda: "Autoconnecting {} to {}".format(_from, _to))
                    self._jack.connect_ports(_from, _to)
                    self.connected.emit()