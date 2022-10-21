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
    slWetLooperChanged = pyqtSignal(QObject)
    slDryLooperChanged = pyqtSignal(QObject)

    def __init__(self, parent=None):
        super(SLFXLooperPairManager, self).__init__(parent)
        self._parent = parent
        self._sl_wet_looper_idx = None
        self._sl_dry_looper_idx = None
        self._sl_wet_looper = None
        self._sl_dry_looper = None
    
    ######################
    # PROPERTIES
    ######################

    @pyqtProperty(int, notify=slDryLooperIdxChanged)
    def sl_dry_looper_idx(self):
        return self._sl_dry_looper_idx

    @sl_dry_looper_idx.setter
    def sl_dry_looper_idx(self, i):
        if i != self._sl_dry_looper_idx:
            if self._sl_dry_looper:
                raise Exception("Changing SL loop idx of already existing looper.")
            self._sl_dry_looper_idx = i
            self.slDryLooperIdxChanged.emit(i)
    
    @pyqtProperty(int, notify=slWetLooperIdxChanged)
    def sl_wet_looper_idx(self):
        return self._sl_wet_looper_idx

    @sl_wet_looper_idx.setter
    def sl_wet_looper_idx(self, i):
        if i != self._sl_wet_looper_idx:
            if self._sl_wet_looper:
                raise Exception("Changing SL loop idx of already existing looper.")
            self._sl_wet_looper_idx = i
            self.slWetLooperIdxChanged.emit(i)
    
    @pyqtProperty(QObject, notify=slDryLooperChanged)
    def sl_dry_looper(self):
        return self._sl_dry_looper
    
    @pyqtProperty(QObject, notify=slWetLooperChanged)
    def sl_wet_looper(self):
        return self._sl_wet_looper
    
    ##################
    # SLOTS / METHODS
    ##################

    def wet(self):
        if self._sl_wet_looper == None:
            if self._sl_wet_looper_idx == None:
                raise Exception("Trying to create SL looper before its index is known")
            self._sl_wet_looper = SLLooperManager(self._parent, self._sl_wet_looper_idx)
            self._sl_wet_looper.sync = self.sync
            # Connections
            self._sl_wet_looper.lengthChanged.connect(self.updateLength)
            self._sl_wet_looper.posChanged.connect(self.updatePos)
            self._sl_wet_looper.stateChanged.connect(self.updateState)
            self.slWetLooperIdxChanged.connect(lambda s: setattr(self._sl_wet_looper, 'sl_looper_index', s))
            self.syncChanged.connect(lambda s: setattr(self._sl_wet_looper, 'sync', s))
            
        return self._sl_wet_looper
    
    def dry(self):
        if self._sl_dry_looper == None:
            if self._sl_dry_looper_idx == None:
                raise Exception("Trying to create SL looper before its index is known")
            self._sl_dry_looper = SLLooperManager(self._parent, self._sl_dry_looper_idx)
            self._sl_dry_looper.sync = self.sync
            # Connections
            self._sl_dry_looper.lengthChanged.connect(self.updateLength)
            self._sl_dry_looper.posChanged.connect(self.updatePos)
            self._sl_dry_looper.stateChanged.connect(self.updateState)
            self.slDryLooperIdxChanged.connect(lambda s: setattr(self._sl_dry_looper, 'sl_looper_index', s))
            self.syncChanged.connect(lambda s: setattr(self._sl_dry_looper, 'sync', s))
            
        return self._sl_dry_looper

    @pyqtSlot()
    def start_sync(self):
        self.dry().start_sync()
        self.wet().start_sync()

    @pyqtSlot()
    def updateLength(self):
        l = max(self.wet().length, self.dry().length)
        if l != self.length:
            self.length = l
            self.lengthChanged.emit(l)
    
    @pyqtSlot()
    def updatePos(self):
        # TODO: better pos combine algo which takes into account the states
        # of the sub-loops
        p = min((self.dry().pos + self.wet().pos)/2.0, self.length)
        if p != self.pos:
            self.pos = p
            self.posChanged.emit(p)
    
    @pyqtSlot()
    def updateState(self):
        # In most cases, our overall state matches that of the wet loop.
        new_state = self.wet().state

        if self.wet().state == LoopState.Recording.value and \
           self.dry().state == LoopState.Playing.value:
           new_state = LoopState.RecordingWet.value
        
        if self.wet().state == LoopState.Muted.value and \
           self.dry().state == LoopState.Playing.value:
           new_state = LoopState.PlayingDryLiveWet.value
        
        if new_state != self.state:
            self.state = new_state
            self.stateChanged.emit(new_state)
    
    @pyqtSlot()
    def doTrigger(self):
        self.dry().doTrigger()
        self.wet().doTrigger()
    
    @pyqtSlot(float, float)
    def setPassthroughs(self, dry, wet):
        self.dry().passthrough = dry
        self.wet().passthrough = wet

    @pyqtSlot()
    def doPlay(self):
        # Mute the dry, because we want to hear
        # the wet only and not process the FX.
        self.dry().doMute()
        self.wet().doPlay()
        self.setPassthroughs(1.0, 1.0)
    
    @pyqtSlot()
    def doPlayLiveFx(self):
        # Mute the wet, play the dry.
        # TODO: wet should still have passthrough
        self.dry().doPlay()
        self.wet().doMute()
        self.setPassthroughs(1.0, 1.0)

    @pyqtSlot()
    def doPause(self):
        self.dry().doPause()
        self.wet().doPause()

    @pyqtSlot()
    def doRecord(self):
        self.dry().doRecord()
        self.wet().doRecord()
        self.setPassthroughs(1.0, 1.0)
    
    @pyqtSlot(QObject)
    def doRecordFx(self, master_manager):
        n_cycles = round(self.length / master_manager.length)
        current_cycle = math.floor(self.pos / master_manager.length)
        wait_cycles_before_start = n_cycles - current_cycle - 1
        def toExecuteJustBeforeRestart():
            self.dry().doPlay()
            self.wet().doRecordNCycles(n_cycles, master_manager)
            # When the FX recording is finished, also mute the
            # dry channel again
            master_manager.schedule_at_loop_pos(
                master_manager.length * 0.7,
                n_cycles, lambda: self.dry().doMute())
            self.setPassthroughs(0.0, 1.0)
        
        if wait_cycles_before_start <= 0:
            toExecuteJustBeforeRestart()
        else:
            master_manager.schedule_at_loop_pos(master_manager.length * 0.7,
                                                     wait_cycles_before_start,
                                                     toExecuteJustBeforeRestart)
        

    @pyqtSlot(int, QObject)
    def doRecordNCycles(self, n, master_manager):
        self.dry().doRecordNCycles(n, master_manager)
        self.wet().doRecordNCycles(n, master_manager)
        self.setPassthroughs(1.0, 1.0)

    @pyqtSlot()
    def doStopRecord(self):
        self.dry().doStopRecord()
        self.wet().doStopRecord()
        self.dry().doMute()

    @pyqtSlot()
    def doMute(self):
        self.dry().doMute()
        self.wet().doMute()

    @pyqtSlot()
    def doPlayDry(self):
        self.dry().doPlay()
        self.wet().doMute()
        self.setPassthroughs(1.0, 1.0)

    @pyqtSlot()
    def doUnmute(self):
        self.dry().doUnmute()
        self.wet().doUnmute()

    @pyqtSlot(str)
    def doLoadWav(self, wav_file):
        # Load the wav into both dry and wet loops.
        # The user can always adjust later.
        self.dry().doLoadWav(wav_file)
        self.wet().doLoadWav(wav_file)

    @pyqtSlot()
    def doClear(self):
        self.dry().doUnMute()
        self.wet().doUnMute()

    @pyqtSlot(str)
    def doSaveWav(self, wav_file):
        raise NotImplementedError()

    @pyqtSlot(QObject)
    def connect_osc_link(self, link):
        self.dry().connect_osc_link(link)
        self.wet().connect_osc_link(link)
