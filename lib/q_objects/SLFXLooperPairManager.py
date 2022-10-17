from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
import re
import time
import os
import wave
import tempfile
from enum import Enum
import math

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
    slDryLooperChanged = pyqtSignal(LooperManager)
    slWetLooperChanged = pyqtSignal(LooperManager)

    def __init__(self, parent=None, sl_dry_looper_idx=None, sl_wet_looper_idx=None):
        super(SLFXLooperPairManager, self).__init__(parent)

        self._parent = parent

        self._sl_dry_looper = SLLooperManager(self._parent, sl_dry_looper_idx if sl_dry_looper_idx != None else 999)
        self._sl_wet_looper = SLLooperManager(self._parent, sl_wet_looper_idx if sl_wet_looper_idx != None else 999)
        self._sl_dry_looper.sync = self.sync
        self._sl_wet_looper.sync = self.sync

        # Connections
        self._sl_dry_looper.lengthChanged.connect(self.updateLength)
        self._sl_dry_looper.posChanged.connect(self.updatePos)
        self._sl_dry_looper.connectedChanged.connect(self.updateConnected)
        self._sl_dry_looper.stateChanged.connect(self.updateState)

        self._sl_wet_looper.lengthChanged.connect(self.updateLength)
        self._sl_wet_looper.posChanged.connect(self.updatePos)
        self._sl_wet_looper.connectedChanged.connect(self.updateConnected)
        self._sl_wet_looper.stateChanged.connect(self.updateState)

        self._sl_dry_looper.slLooperIndexChanged.connect(self.slDryLooperIdxChanged)
        self._sl_wet_looper.slLooperIndexChanged.connect(self.slWetLooperIdxChanged)
        self.syncChanged.connect(lambda s: setattr(self._sl_dry_looper, 'sync', s))
        self.syncChanged.connect(lambda s: setattr(self._sl_wet_looper, 'sync', s))
    
    ######################
    # PROPERTIES
    ######################

    @pyqtProperty(int, notify=slDryLooperIdxChanged)
    def sl_dry_looper_idx(self):
        return self._sl_dry_looper.sl_looper_index

    @sl_dry_looper_idx.setter
    def sl_dry_looper_idx(self, i):
        self._sl_dry_looper.sl_looper_index = i
    
    @pyqtProperty(int, notify=slWetLooperIdxChanged)
    def sl_wet_looper_idx(self):
        return self._sl_wet_looper.sl_looper_index

    @sl_wet_looper_idx.setter
    def sl_wet_looper_idx(self, i):
        self._sl_wet_looper.sl_looper_index = i
    
    @pyqtProperty(QObject, notify=slDryLooperChanged)
    def sl_dry_looper(self):
        return self._sl_dry_looper
    
    @pyqtProperty(QObject, notify=slWetLooperChanged)
    def sl_wet_looper(self):
        return self._sl_wet_looper

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

        if self._sl_wet_looper.state == LoopState.Recording.value and \
           self._sl_dry_looper.state == LoopState.Playing.value:
           new_state = LoopState.RecordingWet.value
        
        if self._sl_wet_looper.state == LoopState.Muted.value and \
           self._sl_dry_looper.state == LoopState.Playing.value:
           new_state = LoopState.PlayingDryLiveWet.value
        
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
        # the wet only and not process the FX.
        self._sl_dry_looper.doMute()
        self._sl_wet_looper.doPlay()
    
    @pyqtSlot()
    def doPlayLiveFx(self):
        # Mute the wet, play the dry.
        # TODO: wet should still have passthrough
        self._sl_dry_looper.doPlay()
        self._sl_wet_looper.doMute()

    @pyqtSlot()
    def doPause(self):
        self._sl_dry_looper.doPause()
        self._sl_wet_looper.doPause()

    @pyqtSlot()
    def doRecord(self):
        self._sl_dry_looper.doRecord()
        self._sl_wet_looper.doRecord()
    
    @pyqtSlot(QObject)
    def doRecordFx(self, master_manager):
        n_cycles = round(self.length / master_manager.length)
        current_cycle = math.floor(self.pos / master_manager.length)
        wait_cycles_before_start = n_cycles - current_cycle - 1
        def toExecuteJustBeforeRestart():
            self._sl_dry_looper.doPlay()
            self._sl_wet_looper.doRecordNCycles(n_cycles, master_manager)
            # When the FX recording is finished, also mute the
            # dry channel again
            master_manager.schedule_at_loop_pos(
                master_manager.length * 0.7,
                n_cycles, lambda: self._sl_dry_looper.doMute())
        
        if wait_cycles_before_start <= 0:
            toExecuteJustBeforeRestart()
        else:
            master_manager.schedule_at_loop_pos(master_manager.length * 0.7,
                                                     wait_cycles_before_start,
                                                     toExecuteJustBeforeRestart)
        

    @pyqtSlot(int, QObject)
    def doRecordNCycles(self, n, master_manager):
        self._sl_dry_looper.doRecordNCycles(n, master_manager)
        self._sl_wet_looper.doRecordNCycles(n, master_manager)

    @pyqtSlot()
    def doStopRecord(self):
        self._sl_dry_looper.doStopRecord()
        self._sl_wet_looper.doStopRecord()
        self._sl_dry_looper.doMute()

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
