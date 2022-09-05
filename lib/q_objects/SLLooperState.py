from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot

from ..LoopState import LoopState

from functools import partial

import pprint

class SLLooperState(QObject):
    def __init__(self, parent=None):
        super(SLLooperState, self).__init__(parent)
        self._length = 1.0
        self._pos = 0.0
        self._connected = False
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
    
    # connected (meaning: loop exists in SL)
    connectedChanged = pyqtSignal(bool)
    @pyqtProperty(bool, notify=connectedChanged)
    def connected(self):
        return self._connected
    @connected.setter
    def connected(self, p):
        if self._connected != p:
            self._connected = p
            self.connectedChanged.emit(p)

    # state: see SL OSC documentation for possible state values
    stateChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=stateChanged)
    def state(self):
        return self._state
    @state.setter
    def state(self, s):
        if self._state != s:
            self._state = s
            self.stateChanged.emit(s)

    @pyqtSlot(QObject)
    def connect_manager(self, manager):
        manager.lengthChanged.connect(partial(SLLooperState.length.fset, self))
        manager.posChanged.connect(partial(SLLooperState.pos.fset, self))
        manager.connectedChanged.connect(partial(SLLooperState.connected.fset, self))
        manager.stateChanged.connect(partial(SLLooperState.state.fset, self))
