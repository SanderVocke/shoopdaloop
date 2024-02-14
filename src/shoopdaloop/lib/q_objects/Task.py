from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQml import QJSValue
from .ShoopPyObject import *

# Simple object that keeps track of an asynchronous task's execution.
class Task(ShoopQObject):
    def __init__(self, parent=None):
        super(Task, self).__init__(parent)
        self._anything_to_do = True

    # anything_to_do
    anythingToDoChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=anythingToDoChanged)
    def anything_to_do(self):
        return self._anything_to_do
    @anything_to_do.setter
    def anything_to_do(self, s):
        if self._anything_to_do != s:
            self._anything_to_do = s
            self.anythingToDoChanged.emit(s)
    
    @ShoopSlot()
    def done(self):
        self.anything_to_do = False

    @ShoopSlot('QVariant')
    def when_finished(self, fn):
        def exec_fn():
            if callable(fn):
                fn()
            elif isinstance(fn, QJSValue):
                fn.call()
        
        if not self._anything_to_do:
            exec_fn()
        else:
            self.anythingToDoChanged.connect(lambda: exec_fn())

