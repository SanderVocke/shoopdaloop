from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from ..logging import Logger as BaseLogger
from ..findFirstParent import findFirstParent

class MidiControlPort(QQuickItem):
    def __init__(self, parent=None):
        super(MidiControlPort, self).__init__(parent)
        self._name_hint = None
        self._direction = None
        self._backend_obj = None
        self.logger = BaseLogger("Frontend.MidiControlPort")
        self._backend = None
        
        self.rescan_parents()
        if not self._backend:
            self.parentChanged.connect(self.rescan_parents)
    
    ######################
    ## SIGNALS
    ######################
    msgReceived = Signal(list)

    ######################
    # PROPERTIES
    ######################

    # name_hint
    nameHintChanged = Signal(str)
    @Property(str, notify=nameHintChanged)
    def name_hint(self):
        return self._name_hint if self._name_hint else ''
    @name_hint.setter
    def name_hint(self, l):
        if l and l != self._name_hint:
            self._name_hint = l
            self.nameHintChanged.emit(l)
            self.maybe_init()
    
    # direction
    directionChanged = Signal(int)
    @Property(int, notify=directionChanged)
    def direction(self):
        return self._direction if self._direction else 0
    @direction.setter
    def direction(self, l):
        if l != self._direction:
            self._direction = l
            self.directionChanged.emit(l)
            self.maybe_init()
    
    # initialized
    initializedChanged = Signal(bool)
    @Property(bool, notify=initializedChanged)
    def initialized(self):
        return self._backend_obj != None
    
    ###########
    ## SLOTS
    ###########

    ###########
    ## METHODS
    ###########
    
    @Slot()
    def poll(self):
        while True:
            r = self._backend_obj.maybe_next_message()
            if not r:
                break
            self.logger.trace("Received: {}".format(r.data))
            self.msgReceived.emit(r.data)
    
    @Slot()
    def rescan_parents(self):
        maybe_backend = findFirstParent(self, lambda p: p and isinstance(p, QQuickItem) and p.inherits('Backend') and self._backend == None)
        if maybe_backend:
            self._backend = maybe_backend
            self.maybe_init()
    
    @Slot()
    def maybe_init(self):
        if self._backend and not self._backend.get_backend_obj():
            self._backend.initializedChanged.connect(self.maybe_init)
            return
        if self._name_hint and self._backend and self._direction != None:
            if self._backend_obj:
                return
            
            self.logger.debug("Opening decoupled MIDI port {}".format(self._name_hint))
            self._backend_obj = self._backend.get_backend_obj().open_decoupled_midi_port(self._name_hint, self._direction)
            
            if not self._backend_obj:
                self.logger.error("Failed to open decoupled MIDI port {}".format(self._name_hint))
                return
            
            self.timer = QTimer(self)
            self.timer.setSingleShot(False)
            self.timer.setInterval(0)
            self.timer.timeout.connect(self.poll)
            self.timer.start()
            
            self.initializedChanged.emit(True)
    