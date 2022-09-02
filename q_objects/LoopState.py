from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty

class LoopState(QObject):
    lengthChanged = pyqtSignal(float)
    posChanged = pyqtSignal(float)

    def __init__(self, parent=None):
        super(LoopManager, self).__init__(parent)
        self._length = 1.0
        self._pos = 0.0
    
    @pyqtProperty(float, notify=lengthChanged)
    def length(self):
        return self._length
    
    @length.setter
    def length(self, l):
        if self._length != l:
            self._length = l
            self.lengthChanged.emit(l)
    
    @pyqtProperty(float, notify=posChanged)
    def pos(self):
        return self._pos
    
    @pos.setter
    def pos(self, p):
        if self._pos != p:
            self._pos = p
            self.posChanged.emit(p)