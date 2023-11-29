from PySide6.QtCore import Signal, Property, Slot, QTimer

from .ShoopPyObject import ShoopQObject
from ..logging import Logger as BaseLogger
from ..logging import _trace, _debug, _info, _warning, _error

class Logger(ShoopQObject):
    def __init__(self, parent=None):
        global logger_id
        global logger_ids_lock
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
        self.logger.trace(msg, _id=self._obj_id)
    
    @Slot('QVariant')
    def debug(self, msg):
        self.logger.debug(msg, _id=self._obj_id)
    
    @Slot('QVariant')
    def info(self, msg):
        self.logger.info(msg, _id=self._obj_id)
    
    @Slot('QVariant')
    def warning(self, msg):
        self.logger.warning(msg, _id=self._obj_id)
    
    @Slot('QVariant')
    def error(self, msg):
        self.logger.error(msg, _id=self._obj_id)
    
    @Slot('QVariant')
    def throw_error(self, msg):
        self.logger.error(msg, _id=self._obj_id)
        raise Exception(resolved if resolved else 'Unknown error')
    