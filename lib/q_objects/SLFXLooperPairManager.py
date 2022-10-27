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
    
    forceWetPassthroughChanged = pyqtSignal(bool)
    forceDryPassthroughChanged = pyqtSignal(bool)

    def __init__(self, parent=None):
        super(SLFXLooperPairManager, self).__init__(parent)
        self._parent = parent
        self._sl_wet_looper_idx = None
        self._sl_dry_looper_idx = None
        self._sl_wet_looper = None
        self._sl_dry_looper = None
        self._force_wet_passthrough = False
        self._force_dry_passthrough = False

        self.passthroughChanged.connect(self.updatePassthroughs)
        self.forceWetPassthroughChanged.connect(self.updatePassthroughs)
        self.forceDryPassthroughChanged.connect(self.updatePassthroughs)

        self.volumeChanged.connect(self.updateVolumes)
        self.panLChanged.connect(self.updatePans)
        self.panRChanged.connect(self.updatePans)
    
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
    
    # internal properties used to relate the loop's overall
    # passthrough setting with the settings that should be
    # passed to the sub-loops. For certain recording states
    # the overall passthrough setting should be overridden
    @pyqtProperty(bool, notify=forceWetPassthroughChanged)
    def force_wet_passthrough(self):
        return self._force_wet_passthrough

    @force_wet_passthrough.setter
    def force_wet_passthrough(self, i):
        if i != self._force_wet_passthrough:
            self._force_wet_passthrough = i
            self.forceWetPassthroughChanged.emit(i)
    
    @pyqtProperty(bool, notify=forceDryPassthroughChanged)
    def force_dry_passthrough(self):
        return self._force_dry_passthrough
    
    @force_dry_passthrough.setter
    def force_dry_passthrough(self, i):
        if i != self._force_dry_passthrough:
            self._force_dry_passthrough = i
            self.forceDryPassthroughChanged.emit(i)
    
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
        self.force_dry_passthrough = False
        self.force_wet_passthrough = False

    @pyqtSlot()
    def updatePassthroughs(self):
        # If a force flag is enabled, that means the passthrough should be active to
        # facilitate a certain mode (e.g. recording).
        self.dry().passthrough = 1.0 if self._force_dry_passthrough or self.passthrough > 0.0 else 0.0
        self.wet().passthrough = self.volume if self._force_wet_passthrough else self.passthrough
    
    @pyqtSlot()
    def updateVolumes(self):
        self.wet().volume = self.volume
        self.dry().volume = 1.0
    
    @pyqtSlot()
    def updatePans(self):
        self.dry().panL = self.panL
        self.dry().panR = self.panR
        self.wet().panL = 0.0
        self.wet().panR = 1.0

    @pyqtSlot()
    def doPlay(self):
        # Mute the dry, because we want to hear
        # the wet only and not process the FX.
        self.dry().doMute()
        self.wet().doPlay()
        self.force_dry_passthrough = False
        self.force_wet_passthrough = False
    
    @pyqtSlot()
    def doPlayLiveFx(self):
        # Mute the wet, play the dry.
        # TODO: wet should still have passthrough
        self.dry().doPlay()
        self.wet().doMute()
        self.force_dry_passthrough = False
        self.force_wet_passthrough = True

    @pyqtSlot()
    def doPause(self):
        self.dry().doPause()
        self.wet().doPause()
        self.force_dry_passthrough = False
        self.force_wet_passthrough = False

    @pyqtSlot()
    def doRecord(self):
        self.dry().doRecord()
        self.wet().doRecord()
        self.force_dry_passthrough = True
        self.force_wet_passthrough = False
    
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
        self.force_dry_passthrough = True
        self.force_wet_passthrough = False

    @pyqtSlot()
    def doStopRecord(self):
        self.dry().doStopRecord()
        self.wet().doStopRecord()
        self.dry().doMute()
        self.force_dry_passthrough = False
        self.force_wet_passthrough = False

    @pyqtSlot()
    def doMute(self):
        self.dry().doMute()
        self.wet().doMute()
        self.force_dry_passthrough = False
        self.force_wet_passthrough = False

    @pyqtSlot()
    def doUnmute(self):
        if self.state == LoopState.Muted.value:
            self.wet().doUnmute()
            self.force_dry_passthrough = False
            self.force_wet_passthrough = False

    @pyqtSlot(str)
    def doLoadWav(self, wav_file):
        # Load the wav into both dry and wet loops.
        # The user can always adjust later.
        self.dry().doLoadWav(wav_file)
        self.wet().doLoadWav(wav_file)

    @pyqtSlot()
    def doClear(self):
        self.dry().doClear()
        self.wet().doClear()
        self.force_dry_passthrough = False
        self.force_wet_passthrough = False

    @pyqtSlot(str)
    def doSaveWav(self, wav_file):
        raise NotImplementedError()

    @pyqtSlot(QObject)
    def connect_osc_link(self, link):
        self.dry().connect_osc_link(link)
        self.wet().connect_osc_link(link)
    
    @pyqtSlot(result=str)
    def looper_type(self):
        return "SLFXLooperPairManager"
