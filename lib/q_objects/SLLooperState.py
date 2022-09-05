from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty

from ..LoopState import LoopState

class SLLooperState(QObject):
    def __init__(self, parent=None):
        super(SLLooperState, self).__init__(parent)
        self._length = 1.0
        self._pos = 0.0
        self._active = False
        self._state = LoopState.Unknown.value
    
    # length: loop length in seconds
    lengthChanged = pyqtSignal(float)
    @pyqtProperty(float, notify=lengthChanged)
    def length(self):
        return self._length
    @length.setter
    def length(self, l):
        if self._length != l:
            self._length = l
            self.lengthChanged.emit(l)
    
    # pos: loop playback position in seconds
    posChanged = pyqtSignal(float)
    @pyqtProperty(float, notify=posChanged)
    def pos(self):
        return self._pos
    @pos.setter
    def pos(self, p):
        if self._pos != p:
            self._pos = p
            self.posChanged.emit(p)
    
    # active (meaning: loop exists in SL)
    activeChanged = pyqtSignal(bool)
    @pyqtProperty(bool, notify=activeChanged)
    def active(self):
        return self._active
    @active.setter
    def active(self, p):
        if self._active != p:
            self._active = p
            self.activeChanged.emit(p)

    # state: see SL OSC documentation for possible state values
    stateChanged = pyqtSignal(int)
    @pyqtProperty(str, notify=stateChanged)
    def state(self):
        return self._state
    @state.setter
    def state(self, s):
        if self._state != s:
            self._state = s
            self.stateChanged.emit(s)
