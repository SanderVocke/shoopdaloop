from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer

# Simple object that keeps track of an asynchronous task's execution.
class Task(QObject):
    def __init__(self, parent=None):
        super(Task, self).__init__(parent)
        self._anything_to_do = True

    # anything_to_do
    anythingToDoChanged = pyqtSignal(bool)
    @pyqtProperty(bool, notify=anythingToDoChanged)
    def anything_to_do(self):
        return self._anything_to_do
    @anything_to_do.setter
    def anything_to_do(self, s):
        if self._anything_to_do != s:
            self._anything_to_do = s
            self.anythingToDoChanged.emit(s)
    
    @pyqtSlot()
    def done(self):
        self.anything_to_do = False
