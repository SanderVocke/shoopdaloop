from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQml import QJSValue

# Simple object that keeps track of an asynchronous task's execution.
class Task(QObject):
    def __init__(self, parent=None):
        super(Task, self).__init__(parent)
        self._anything_to_do = True

    # anything_to_do
    anythingToDoChanged = Signal(bool)
    @Property(bool, notify=anythingToDoChanged)
    def anything_to_do(self):
        return self._anything_to_do
    @anything_to_do.setter
    def anything_to_do(self, s):
        if self._anything_to_do != s:
            self._anything_to_do = s
            self.anythingToDoChanged.emit(s)
    
    @Slot()
    def done(self):
        self.anything_to_do = False

    @Slot('QVariant')
    def when_finished(self, fn):
        def exec_fn():
            if callable(fn):
                print("exec callable")
                fn()
            elif isinstance(fn, QJSValue):
                print("exec qjsvalue")
                fn.call()
        
        if not self._anything_to_do:
            exec_fn()
        else:
            self.anythingToDoChanged.connect(lambda: exec_fn())

