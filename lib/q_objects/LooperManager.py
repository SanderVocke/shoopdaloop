from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
import re
import time
import os
import wave
import tempfile

from ..LoopState import LoopState, LoopActionType

# Base class for loopers to be used by ShoopDaLoop.
class LooperManager(QObject):

    # State change notifications
    lengthChanged = pyqtSignal(float)
    posChanged = pyqtSignal(float)
    stateChanged = pyqtSignal(int)
    syncChanged = pyqtSignal(bool)
    volumeChanged = pyqtSignal(float)
    panRChanged = pyqtSignal(float)
    panLChanged = pyqtSignal(float)

    def __init__(self, parent=None):
        super(LooperManager, self).__init__(parent)
        self._length = 1.0
        self._pos = 0.0
        self._state = LoopState.Unknown.value
        self._last_received_pos_t = None
        self._sync = False
        self._volume = 1.0
        self._panL = 0.0
        self._panR = 1.0

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
    
    # sync (whether this loop is synced to the main loop or not)
    @pyqtProperty(bool, notify=syncChanged)
    def sync(self):
        return self._sync

    @sync.setter
    def sync(self, c):
        if self._sync != c:
            self._sync = c
            self.syncChanged.emit(c)
    
    # Volume: volume of playback in 0.0-1.0
    @pyqtProperty(float, notify=volumeChanged)
    def volume(self):
        return self._volume
    
    @volume.setter
    def volume(self, p):
        if self._volume != p:
            self._volume = p
            self.volumeChanged.emit(p)
    
    # Pan properties: panning of each channel in 0.0-1.0
    @pyqtProperty(float, notify=panLChanged)
    def panL(self):
        return self._panL
    
    @panL.setter
    def panL(self, p):
        if self._panL != p:
            self._panL = p
            self.panLChanged.emit(p)

    @pyqtProperty(float, notify=panRChanged)
    def panR(self):
        return self._panR
    
    @panR.setter
    def panR(self, p):
        if self._panR != p:
            self._panR = p
            self.panRChanged.emit(p)

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
    
    @pyqtSlot(int, list)
    def doLoopAction(self, action_id, args):
        raise NotImplementedError()
    
    @pyqtSlot(result=str)
    def looper_type(self):
        raise NotImplementedError()
    
    @pyqtSlot(QObject, int, int)
    def connect_midi_control_manager(self, manager, track_idx, loop_idx):
        if manager:
            self.stateChanged.connect(lambda state: manager.loop_state_changed(track_idx, loop_idx, state))
            self.stateChanged.emit(self.state)
    
    @pyqtSlot(QObject)
    def connect_backend_manager(self, manager):
        raise NotImplementedError()

