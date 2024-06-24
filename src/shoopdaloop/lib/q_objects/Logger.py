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
    @ShoopProperty(str, notify=nameChanged, thread_protection=ThreadProtectionType.AnyThread)
    def name(self):
        return self.logger.name()
    @name.setter
    def name(self, l):
        if l and l != self.logger.name():
            self.logger = BaseLogger(l)
    
    # instanceIdentifier (for clarity in debugging)
    instanceIdentifierChanged = ShoopSignal(str)
    @ShoopProperty(str, notify=instanceIdentifierChanged, thread_protection=ThreadProtectionType.AnyThread)
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

    @ShoopSlot('QVariant', thread_protection=ThreadProtectionType.AnyThread)
    def trace(self, msg):
        self.logger.trace(msg, _id=self._id_to_print)
    
    @ShoopSlot('QVariant', thread_protection=ThreadProtectionType.AnyThread)
    def debug(self, msg):
        self.logger.debug(msg, _id=self._id_to_print)
    
    @ShoopSlot('QVariant', thread_protection=ThreadProtectionType.AnyThread)
    def info(self, msg):
        self.logger.info(msg, _id=self._id_to_print)
    
    @ShoopSlot('QVariant', thread_protection=ThreadProtectionType.AnyThread)
    def warning(self, msg):
        self.logger.warning(msg, _id=self._id_to_print)
    
    @ShoopSlot('QVariant', thread_protection=ThreadProtectionType.AnyThread)
    def error(self, msg):
        self.logger.error(msg, _id=self._id_to_print)
    
    @ShoopSlot('QVariant', thread_protection=ThreadProtectionType.AnyThread)
    def throw_error(self, msg):
        self.logger.error(msg, _id=self._id_to_print)
        raise Exception(msg)
    
    @ShoopSlot(thread_protection=ThreadProtectionType.AnyThread)
    def should_trace(self):
        return self.logger.should_trace()
    
    @ShoopSlot(thread_protection=ThreadProtectionType.AnyThread)
    def should_debug(self):
        return self.logger.should_debug()
