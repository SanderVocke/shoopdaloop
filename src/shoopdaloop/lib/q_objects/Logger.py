from PySide6.QtCore import Signal, Property, Slot, QTimer

from .ShoopPyObject import *
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
    nameChanged = ShoopSignal(str)
    @ShoopProperty(str, notify=nameChanged, thread_protected=False)
    def name(self):
        return self.logger.name()
    @name.setter
    def name(self, l):
        if l and l != self.logger.name():
            self.logger = BaseLogger(l)
    
    # instanceIdentifier (for clarity in debugging)
    instanceIdentifierChanged = ShoopSignal(str)
    @ShoopProperty(str, notify=instanceIdentifierChanged, thread_protected=False)
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

    @ShoopSlot('QVariant', thread_protected=False)
    def trace(self, msg):
        self.logger.trace(msg, _id=self._id_to_print)
    
    @ShoopSlot('QVariant', thread_protected=False)
    def debug(self, msg):
        self.logger.debug(msg, _id=self._id_to_print)
    
    @ShoopSlot('QVariant', thread_protected=False)
    def info(self, msg):
        self.logger.info(msg, _id=self._id_to_print)
    
    @ShoopSlot('QVariant', thread_protected=False)
    def warning(self, msg):
        self.logger.warning(msg, _id=self._id_to_print)
    
    @ShoopSlot('QVariant', thread_protected=False)
    def error(self, msg):
        self.logger.error(msg, _id=self._id_to_print)
    
    @ShoopSlot('QVariant', thread_protected=False)
    def throw_error(self, msg):
        self.logger.error(msg, _id=self._id_to_print)
        raise Exception(msg)
