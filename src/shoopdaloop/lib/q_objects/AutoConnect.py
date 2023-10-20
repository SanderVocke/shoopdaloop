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
        if self._from_regex is None or self._to_regex is None:
            return
        
        from_candidates = self._jack.find_ports(self._from_regex, None, jack.JackPortIsOutput)
        to_candidates = self._jack.find_ports(self._to_regex, None, jack.JackPortIsInput)
        
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