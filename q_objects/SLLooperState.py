from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot

class SLLooperState(QObject):
    lengthChanged = pyqtSignal(float)
    posChanged = pyqtSignal(float)
    activeChanged = pyqtSignal(bool)

    def __init__(self, parent=None):
        super(SLLooperState, self).__init__(parent)
        self._length = 1.0
        self._pos = 0.0
        self._active = False
    
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
    
    @pyqtProperty(bool, notify=activeChanged)
    def active(self):
        return self._active
    
    @active.setter
    def active(self, p):
        if self._active != p:
            self._active = p
            self.activeChanged.emit(p)