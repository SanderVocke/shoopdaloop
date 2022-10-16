from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
import re
import time
import os
import wave
import tempfile
from enum import Enum

from ..LoopState import LoopState
from .SLLooperManager import SLLooperManager
from .LooperManager import LooperManager

# Combines two Sooperlooper loops into a single looper interface which
# offers an FX send/return by using one loop for the dry and one for the
# wet signal.
class SLFXLooperPairManager(LooperManager):

    # State change notifications
    slDryLooperIdxChanged = pyqtSignal(int)
    slWetLooperIdxChanged = pyqtSignal(int)

    def __init__(self, parent=None, sl_dry_looper_idx=None, sl_wet_looper_idx=None):
        super(SLFXLooperPairManager, self).__init__(parent)

        self._parent = parent
        
        self._sl_dry_looper_idx = sl_dry_looper_idx
        self._sl_wet_looper_idx = sl_wet_looper_idx

        self._sl_dry_looper = SLLooperManager(self._parent, self.sl_dry_looper_idx if self.sl_dry_looper_idx != None else 999)
        self._sl_wet_looper = SLLooperManager(self._parent, self.sl_wet_looper_idx if self.sl_wet_looper_idx != None else 999)

        # Connections
        self._sl_dry_looper.lengthChanged.connect(self.updateLength)
        self._sl_dry_looper.posChanged.connect(self.updatePos)
        self._sl_dry_looper.connectedChanged.connect(self.updateConnected)
        self._sl_dry_looper.stateChanged.connect(self.updateState)

        self._sl_wet_looper.lengthChanged.connect(self.updateLength)
        self._sl_wet_looper.posChanged.connect(self.updatePos)
        self._sl_wet_looper.connectedChanged.connect(self.updateConnected)
        self._sl_wet_looper.stateChanged.connect(self.updateState)
    
    ######################
    # PROPERTIES
    ######################

    @pyqtProperty(int, notify=slDryLooperIdxChanged)
    def sl_dry_looper_idx(self):
        return self._sl_dry_looper_idx

    @sl_dry_looper_idx.setter
    def sl_dry_looper_idx(self, i):
        if self._sl_dry_looper_idx != i:
            self._sl_dry_looper_idx = i
            self._sl_dry_looper.sl_looper_index = i
            self.slDryLooperIdxChanged.emit(i)
    
    @pyqtProperty(int, notify=slWetLooperIdxChanged)
    def sl_wet_looper_idx(self):
        return self._sl_wet_looper_idx

    @sl_wet_looper_idx.setter
    def sl_wet_looper_idx(self, i):
        if self._sl_wet_looper_idx != i:
            self._sl_wet_looper_idx = i
            self._sl_wet_looper.sl_looper_index = i
            self.slWetLooperIdxChanged.emit(i)

    ##################
    # SLOTS / METHODS
    ##################

    @pyqtSlot()
    def start_sync(self):
        self._sl_dry_looper.start_sync()
        self._sl_wet_looper.start_sync()

    @pyqtSlot()
    def updateLength(self):
        l = max(self._sl_wet_looper.length, self._sl_dry_looper.length)
        if l != self.length:
            self.length = l
            self.lengthChanged.emit(l)
    
    @pyqtSlot()
    def updatePos(self):
        p = min((self._sl_dry_looper.pos + self._sl_wet_looper.pos)/2.0, self.length)
        if p != self.pos:
            self.pos = p
            self.posChanged.emit(p)
    
    @pyqtSlot()
    def updateConnected(self):
        c = self._sl_dry_looper.connected and self._sl_wet_looper.connected
        if c != self.connected:
            self.connected = c
            self.connectedChanged.emit(c)
    
    @pyqtSlot()
    def updateState(self):
        # In most cases, our overall state matches that of the wet loop.
        new_state = self._sl_wet_looper.state

        if self._sl_wet_looper.state == LoopState.Recording and \
           self._sl_dry_looper.state == LoopState.Playing:
           new_state = LoopState.RecordingWet
        
        if self._sl_wet_looper.state == LoopState.Muted and \
           self._sl_dry_looper.state == LoopState.Playing:
           new_state = LoopState.PlayingDryLiveWet
        
        if new_state != self.state:
            self.state = new_state
            self.stateChanged.emit(new_state)
    
    @pyqtSlot()
    def doTrigger(self):
        self._sl_dry_looper.doTrigger()
        self._sl_wet_looper.doTrigger()

    @pyqtSlot()
    def doPlay(self):
        # Mute the dry, because we want to hear
        # the wet only.
        self._sl_dry_looper.doMute()
        self._sl_wet_looper.doPlay()

    @pyqtSlot()
    def doPause(self):
        self._sl_dry_looper.doPause()
        self._sl_wet_looper.doPause()

    @pyqtSlot()
    def doRecord(self):
        self._sl_dry_looper.doRecord()
        self._sl_wet_looper.doRecord()

    @pyqtSlot(int, QObject)
    def doRecordNCycles(self, n, master_manager):
        self._sl_dry_looper.doRecordNCycles(n, master_manager)
        self._sl_wet_looper.doRecord(n, master_manager)

    @pyqtSlot()
    def doStopRecord(self):
        self._sl_dry_looper.doStopRecord()
        self._sl_wet_looper.doStopRecord()

    @pyqtSlot()
    def doMute(self):
        self._sl_dry_looper.doMute()
        self._sl_wet_looper.doMute()

    @pyqtSlot()
    def doUnmute(self):
        self._sl_dry_looper.doUnmute()
        self._sl_wet_looper.doUnmute()

    @pyqtSlot(str)
    def doLoadWav(self, wav_file):
        raise NotImplementedError()

    @pyqtSlot()
    def doClear(self):
        self._sl_dry_looper.doUnMute()
        self._sl_wet_looper.doUnMute()

    @pyqtSlot(str)
    def doSaveWav(self, wav_file):
        raise NotImplementedError()

    @pyqtSlot(QObject)
    def connect_osc_link(self, link):
        self._sl_dry_looper.connect_osc_link(link)
        self._sl_wet_looper.connect_osc_link(link)
