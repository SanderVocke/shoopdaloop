from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
import re
import time
import os
import wave
import tempfile

from ..LoopState import LoopState

# Base class for loopers to be used by ShoopDaLoop.
class LooperManager(QObject):

    # State change notifications
    lengthChanged = pyqtSignal(float)
    posChanged = pyqtSignal(float)
    connectedChanged = pyqtSignal(bool)
    stateChanged = pyqtSignal(int)
    syncChanged = pyqtSignal(bool)

    def __init__(self, parent=None):
        super(LooperManager, self).__init__(parent)
        self._length = 1.0
        self._pos = 0.0
        self._connected = False
        self._state = LoopState.Unknown.value
        self._last_received_pos_t = None
        self._sync = False

    ######################
    # PROPERTIES
    ######################

    # state
    stateChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=stateChanged)
    def state(self):
        return self._state
    @state.setter
    def state(self, s):
        if self._state != s:
            self._state = s
            self.stateChanged.emit(s)

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
        self._last_received_pos_t = time.monotonic()
        if self._pos != p:
            self._pos = p
            self.posChanged.emit(p)

    # connected (meaning: loop(s) exist(s))
    connectedChanged = pyqtSignal(bool)
    @pyqtProperty(bool, notify=connectedChanged)
    def connected(self):
        return self._connected
    @connected.setter
    def connected(self, p):
        if self._connected != p:
            self._connected = p
            self.connectedChanged.emit(p)
    
    # sync (whether this loop is to be synced to the main loop or not)
    @pyqtProperty(bool, notify=syncChanged)
    def sync(self):
        return self._sync

    @sync.setter
    def sync(self, c):
        if self._sync != c:
            self._sync = c
            self.syncChanged.emit(c)

    ##################
    # SLOTS / METHODS
    ##################

    def schedule_at_loop_pos(self, pos, n_cycles_ahead, action_fn):
        if self.state in [LoopState.Off.value, LoopState.Unknown.value, LoopState.Paused.value]:
            # must be running
            return

        if self._last_received_pos_t == None:
            # no pos gotten yet
            return

        estimated_pos = time.monotonic() - self._last_received_pos_t + self.pos
        if self.pos > estimated_pos and n_cycles_ahead == 0:
            # passed that point already
            return

        milliseconds_ahead = int((pos - estimated_pos + self.length * n_cycles_ahead) * 1000.0)
        QTimer.singleShot(milliseconds_ahead, action_fn)
    
    @pyqtSlot()
    def doTrigger(self):
        raise NotImplementedError()

    @pyqtSlot()
    def doPlay(self):
        raise NotImplementedError()
    
    @pyqtSlot()
    def doPlayLiveFx(self):
        raise NotImplementedError()

    @pyqtSlot()
    def doPause(self):
        raise NotImplementedError()

    @pyqtSlot()
    def doRecord(self):
        raise NotImplementedError()
    
    @pyqtSlot()
    def doRecordFx(self, master_manager):
        raise NotImplementedError()

    @pyqtSlot(int, QObject)
    def doRecordNCycles(self, n, master_manager):
        raise NotImplementedError()

    @pyqtSlot()
    def doStopRecord(self):
        raise NotImplementedError()

    @pyqtSlot()
    def doMute(self):
        raise NotImplementedError()

    @pyqtSlot()
    def doUnmute(self):
        raise NotImplementedError()

    @pyqtSlot()
    def doInsert(self):
        raise NotImplementedError()

    @pyqtSlot()
    def doReplace(self):
        raise NotImplementedError()

    @pyqtSlot(str)
    def doLoadWav(self, wav_file):
        raise NotImplementedError()

    @pyqtSlot()
    def doClear(self):
        raise NotImplementedError()

    @pyqtSlot(str)
    def doSaveWav(self, wav_file):
        raise NotImplementedError()

    @pyqtSlot(QObject)
    def connect_osc_link(self, link):
        raise NotImplementedError()
