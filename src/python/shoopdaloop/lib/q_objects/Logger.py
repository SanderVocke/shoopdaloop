from PySide6.QtCore import Signal, Property, Slot, QTimer
from .ShoopPyObject import *
from .LoggerImpl import LoggerImpl
class Logger(ShoopQObject):
    def __init__(self, parent=None):
        super(Logger, self).__init__(parent)
        self.impl = LoggerImpl(self._obj_id)

    ######################
    # PROPERTIES
    ######################

    # name
    nameChanged = ShoopSignal(str)
    @ShoopProperty(str, notify=nameChanged, thread_protection=ThreadProtectionType.AnyThread)
    def name(self):
        return self.impl.get_name()
    @name.setter
    def name(self, l):
        if self.impl.set_name(l):
            self.nameChanged.emit(l)

    # instanceIdentifier (for clarity in debugging)
    instanceIdentifierChanged = ShoopSignal(str)
    @ShoopProperty(str, notify=instanceIdentifierChanged, thread_protection=ThreadProtectionType.AnyThread)
    def instanceIdentifier(self):
        return self.impl.get_instance_identifier()
    @instanceIdentifier.setter
    def instanceIdentifier(self, l):
        if self.impl.set_instance_identifier(l):
            self.instanceIdentifierChanged.emit(l)
        
    ###########
    ## SLOTS
    ###########

    @ShoopSlot('QString', thread_protection=ThreadProtectionType.AnyThread)
    def trace(self, msg):
        self.impl.trace(msg)
    
    @ShoopSlot('QString', thread_protection=ThreadProtectionType.AnyThread)
    def debug(self, msg):
        self.impl.debug(msg)
    
    @ShoopSlot('QString', thread_protection=ThreadProtectionType.AnyThread)
    def info(self, msg):
        self.impl.info(msg)
    
    @ShoopSlot('QString', thread_protection=ThreadProtectionType.AnyThread)
    def warning(self, msg):
        self.impl.warning(msg)
    
    @ShoopSlot('QString', thread_protection=ThreadProtectionType.AnyThread)
    def error(self, msg):
        self.impl.error(msg)
    
    @ShoopSlot('QString', thread_protection=ThreadProtectionType.AnyThread)
    def throw_error(self, msg):
        self.impl.error(msg)
    
    @ShoopSlot(result=bool, thread_protection=ThreadProtectionType.AnyThread)
    def should_trace(self):
        return self.impl.should_trace()
    
    @ShoopSlot(result=bool, thread_protection=ThreadProtectionType.AnyThread)
    def should_debug(self):
        return self.impl.should_debug()
