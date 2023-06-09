from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from ..logging import Logger as BaseLogger

class Logger(QObject):
    def __init__(self, parent=None):
        super(Logger, self).__init__(parent)
        self._logger = BaseLogger("Frontend.Unknown")

    ######################
    # PROPERTIES
    ######################

    # name
    nameChanged = Signal(str)
    @Property(str, notify=nameChanged)
    def name(self):
        return self._logger.name()
    @name.setter
    def name(self, l):
        if l and l != self._logger.name():
            self._logger = BaseLogger(l)
    
    ###########
    ## SLOTS
    ###########

    @Slot(str)
    def trace(self, msg):
        self._logger.trace(msg)
    
    @Slot(str)
    def debug(self, msg):
        self._logger.debug(msg)
    
    @Slot(str)
    def info(self, msg):
        self._logger.info(msg)
    
    @Slot(str)
    def warning(self, msg):
        self._logger.warning(msg)
    
    @Slot(str)
    def error(self, msg):
        self._logger.error(msg)
    