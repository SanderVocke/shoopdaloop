from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQml import QJSValue
from PySide6.QtQuick import QQuickItem

from ..logging import Logger

class TestCase(QQuickItem):
    def __init__(self, parent=None):
        super(TestCase, self).__init__(parent)
        self._name = ''
    
    run_signal = Signal()
    wait_signal = Signal(int)
    
    # name
    nameChanged = Signal(str)
    @Property(str, notify=nameChanged)
    def name(self):
        return self._name
    @name.setter
    def name(self, s):
        if self._name != s:
            self._name = s
            self.nameChanged.emit(s)

    @Slot()
    def run(self):
        self.run_signal.emit()
        
    @Slot(int)
    def wait(self, ms):
        self.wait_signal.emit(ms)
            
