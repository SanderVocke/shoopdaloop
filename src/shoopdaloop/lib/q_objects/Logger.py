from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from ..logging import Logger as BaseLogger

from threading import Lock
logger_ids_lock = Lock()
logger_id = 1

class Logger(QObject):
    def __init__(self, parent=None):
        global logger_id
        global logger_ids_lock
        super(Logger, self).__init__(parent)
        self.logger = BaseLogger("Frontend.Unknown")
        logger_ids_lock.acquire()
        self._id = logger_id
        logger_id += 1
        logger_ids_lock.release()
        

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
        self.logger.trace('[@{}] {}'.format(self._id, msg))
    
    @Slot('QVariant')
    def debug(self, msg):
        self.logger.debug('[@{}] {}'.format(self._id, msg))
    
    @Slot('QVariant')
    def info(self, msg):
        self.logger.info('[@{}] {}'.format(self._id, msg))
    
    @Slot('QVariant')
    def warning(self, msg):
        self.logger.warning('[@{}] {}'.format(self._id, msg))
    
    @Slot('QVariant')
    def error(self, msg):
        self.logger.error('[@{}] {}'.format(self._id, msg))
    
    @Slot('QVariant')
    def throw_error(self, msg):
        self.logger.error(msg)
        raise Exception(msg)
    