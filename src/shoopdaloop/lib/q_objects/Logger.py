from PySide6.QtCore import Signal, Property, Slot, QTimer

from .ShoopPyObject import ShoopQObject
from ..logging import Logger as BaseLogger

class Logger(ShoopQObject):
    def __init__(self, parent=None):
        global logger_id
        global logger_ids_lock
        super(Logger, self).__init__(parent)
        self.logger = BaseLogger("Frontend.Unknown")
        self._id = None
        self._id_to_print = self._obj_id

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
    
    # instanceIdentifier (for clarity in debugging)
    instanceIdentifierChanged = Signal(str)
    @Property(str, notify=instanceIdentifierChanged)
    def instanceIdentifier(self):
        return self._id
    @instanceIdentifier.setter
    def instanceIdentifier(self, l):
        if l and l != self._id:
            self._id = l
            self._id_to_print = str(self._obj_id) + ":" + l
            self.instanceIdentifierChanged.emit(l)
        
    ###########
    ## SLOTS
    ###########

    @Slot('QVariant')
    def trace(self, msg):
        self.logger.trace(msg, _id=self._id_to_print)
    
    @Slot('QVariant')
    def debug(self, msg):
        self.logger.debug(msg, _id=self._id_to_print)
    
    @Slot('QVariant')
    def info(self, msg):
        self.logger.info(msg, _id=self._id_to_print)
    
    @Slot('QVariant')
    def warning(self, msg):
        self.logger.warning(msg, _id=self._id_to_print)
    
    @Slot('QVariant')
    def error(self, msg):
        self.logger.error(msg, _id=self._id_to_print)
    
    @Slot('QVariant')
    def throw_error(self, msg):
        self.logger.error(msg, _id=self._id_to_print)
        raise Exception(msg)
    