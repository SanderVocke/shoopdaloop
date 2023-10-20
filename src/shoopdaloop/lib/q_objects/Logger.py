from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from ..logging import Logger as BaseLogger

class Logger(QObject):
    def __init__(self, parent=None):
        super(Logger, self).__init__(parent)
        self.logger = BaseLogger("Frontend.Unknown")

    ######################
    # PROPERTIES
    ######################

    # name
    nameChanged = Signal(str)
    @Property(str, notify=nameChanged)
    def name(self):
        return self.logger.name()
    @name.setter
    def name(self, l):
        if l and l != self.logger.name():
            self.logger = BaseLogger(l)
    
    ###########
    ## SLOTS
    ###########

    @Slot('QVariant')
    def trace(self, msg):
        self.logger.trace(msg)
    
    @Slot('QVariant')
    def debug(self, msg):
        self.logger.debug(msg)
    
    @Slot('QVariant')
    def info(self, msg):
        self.logger.info(msg)
    
    @Slot('QVariant')
    def warning(self, msg):
        self.logger.warning(msg)
    
    @Slot('QVariant')
    def error(self, msg):
        self.logger.error(msg)
    
    @Slot('QVariant')
    def throw_error(self, msg):
        self.logger.error(msg)
        raise Exception(msg)
    