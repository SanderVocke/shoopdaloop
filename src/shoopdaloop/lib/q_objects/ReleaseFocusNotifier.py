from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer

from .ShoopPyObject import *
class ReleaseFocusNotifier(ShoopQObject):
    def __init__(self, parent=None):
        super(ReleaseFocusNotifier, self).__init__(parent)
    
    focusReleased = Signal()

    @Slot()
    def notify(self):
        self.focusReleased.emit()

