from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
import re
import time
import os
import wave
import tempfile
from enum import Enum
import math

from ..LoopState import LoopState
from .BackendLooperManager import BackendLooperManager
from .LooperManager import LooperManager

# Combines two loops into a single looper interface which
# offers an FX send/return by using one loop for the dry and one for the
# wet signal.
class BackendFXLooperPairManager(LooperManager):

    # State change notifications
    dryLooperIdxChanged = pyqtSignal(int)
    wetLooperIdxChanged = pyqtSignal(int)
    wetLooperChanged = pyqtSignal(QObject)
    dryLooperChanged = pyqtSignal(QObject)

    # To achieve certain recording/playback states, it is sometimes
    # necessary to override the dry or the wet looper's ports to have
    # passthrough. The pair manager requests this using these signals.
    forceWetPassthroughChanged = pyqtSignal(bool)
    forceDryPassthroughChanged = pyqtSignal(bool)

    def __init__(self, parent=None):
        super(BackendFXLooperPairManager, self).__init__(parent)
        self._parent = parent
        self._wet_looper_idx = None
        self._dry_looper_idx = None
        self._wet_looper = None
        self._dry_looper = None
        self._force_wet_passthrough = False
        self._force_dry_passthrough = False

        self.forceWetPassthroughChanged.connect(self.updatePassthroughs)
        self.forceDryPassthroughChanged.connect(self.updatePassthroughs)

        self.volumeChanged.connect(self.updateVolumes)
        self.panLChanged.connect(self.updatePans)
        self.panRChanged.connect(self.updatePans)
    
    ######################
    # PROPERTIES
    ######################

    @pyqtProperty(int, notify=dryLooperIdxChanged)
    def dry_looper_idx(self):
        return self._dry_looper_idx

    @dry_looper_idx.setter
    def dry_looper_idx(self, i):
        if i != self._dry_looper_idx:
            if self._dry_looper:
                raise Exception("Changing loop idx of already existing looper.")
            self._dry_looper_idx = i
            self.dryLooperIdxChanged.emit(i)
    
    @pyqtProperty(int, notify=wetLooperIdxChanged)
    def wet_looper_idx(self):
        return self._wet_looper_idx

    @wet_looper_idx.setter
    def wet_looper_idx(self, i):
        if i != self._wet_looper_idx:
            if self._wet_looper:
                raise Exception("Changing SL loop idx of already existing looper.")
            self._wet_looper_idx = i
            self.wetLooperIdxChanged.emit(i)
    
    @pyqtProperty(QObject, notify=dryLooperChanged)
    def dry_looper(self):
        return self._dry_looper
    
    @pyqtProperty(QObject, notify=wetLooperChanged)
    def wet_looper(self):
        return self._wet_looper
    
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
        if self._wet_looper == None:
            if self._wet_looper_idx == None:
                raise Exception("Trying to create looper before its index is known")
            self._wet_looper = BackendLooperManager(self._parent, self._wet_looper_idx)
            self._wet_looper.sync = False if self._wet_looper_idx == 0 else self.sync
            # Connections
            self._wet_looper.lengthChanged.connect(self.updateLength)
            self._wet_looper.posChanged.connect(self.updatePos)
            self._wet_looper.stateChanged.connect(self.updateState)
            self.wetLooperIdxChanged.connect(lambda s: setattr(self._wet_looper, 'loop_idx', s))
            self.syncChanged.connect(lambda s: setattr(self._wet_looper, 'sync', s))
            
        return self._wet_looper
    
    def dry(self):
        if self._dry_looper == None:
            if self._dry_looper_idx == None:
                raise Exception("Trying to create looper before its index is known")
            self._dry_looper = BackendLooperManager(self._parent, self._dry_looper_idx)
            self._dry_looper.sync = False if self._dry_looper_idx == 0 else self.sync
            # Connections
            self._dry_looper.lengthChanged.connect(self.updateLength)
            self._dry_looper.posChanged.connect(self.updatePos)
            self._dry_looper.stateChanged.connect(self.updateState)
            self.dryLooperIdxChanged.connect(lambda s: setattr(self._dry_looper, 'loop_idx', s))
            self.syncChanged.connect(lambda s: setattr(self._dry_looper, 'sync', s))
            
        return self._dry_looper

    @pyqtSlot()
    def updateLength(self):
        self.length = max(self.wet().length, self.dry().length)
    
    @pyqtSlot()
    def updatePos(self):
        # In most cases, we want to show the position of the wet
        # loop.        
        if self.wet().state == LoopState.Stopped.value and \
            self.dry().state == LoopState.Playing.value:
            # Playing with live FX. show the pos of the dry loop in this case
            self.pos = self.dry().pos
        else:
            self.pos = self.wet().pos
    
    @pyqtSlot()
    def updateState(self):
        # In most cases, our overall state matches that of the wet loop.
        new_state = self.wet().state

        if self.wet().state == LoopState.Recording.value and \
           self.dry().state == LoopState.Playing.value:
           new_state = LoopState.RecordingFX.value
        
        if self.wet().state == LoopState.Stopped.value and \
           self.dry().state == LoopState.Playing.value:
           new_state = LoopState.PlayingLiveFX.value
        
        if new_state != self.state:
            self.state = new_state
            self.stateChanged.emit(new_state)
    
    @pyqtSlot()
    def doTrigger(self):
        self.wet().doTrigger()
        self.dry().doTrigger()
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
        self.wet().doPlay()
        self.dry().doMute()
        self.force_dry_passthrough = False
        self.force_wet_passthrough = False
    
    @pyqtSlot()
    def doPlayLiveFx(self):
        # Mute the wet, play the dry.
        # TODO: wet should still have passthrough
        self.wet().doMute()
        self.dry().doPlay()
        self.force_dry_passthrough = False
        self.force_wet_passthrough = True

    @pyqtSlot()
    def doPause(self):
        self.wet().doPause()
        self.dry().doPause()
        self.force_dry_passthrough = False
        self.force_wet_passthrough = False

    @pyqtSlot()
    def doRecord(self):
        self.wet().doRecord()
        self.dry().doRecord()
        self.force_dry_passthrough = True
        self.force_wet_passthrough = False
    
    @pyqtSlot(QObject)
    def doRecordFx(self, master_manager):
        n_cycles = round(self.length / master_manager.length)
        current_cycle = math.floor(self.pos / master_manager.length)
        wait_cycles_before_start = n_cycles - current_cycle - 1
        def toExecuteJustBeforeRestart():
            self.wet().doRecordNCycles(n_cycles, master_manager)
            self.dry().doPlay()
            # When the FX recording is finished, also mute the
            # dry channel again
            master_manager.schedule_at_loop_pos(
                master_manager.length * 0.7,
                n_cycles, lambda: self.dry().doMute())
            self.force_dry_passthrough = False
            self.force_wet_passthrough = False
        
        if wait_cycles_before_start <= 0:
            toExecuteJustBeforeRestart()
        else:
            master_manager.schedule_at_loop_pos(master_manager.length * 0.7,
                                                     wait_cycles_before_start,
                                                     toExecuteJustBeforeRestart)
        

    @pyqtSlot(int, QObject)
    def doRecordNCycles(self, n, master_manager):
        self.wet().doRecordNCycles(n, master_manager)
        self.dry().doRecordNCycles(n, master_manager)
        self.force_dry_passthrough = True
        self.force_wet_passthrough = False

    @pyqtSlot()
    def doStopRecord(self):
        self.wet().doStopRecord()
        self.dry().doStopRecord()
        self.dry().doMute()
        self.force_dry_passthrough = False
        self.force_wet_passthrough = False

    @pyqtSlot()
    def doMute(self):
        self.wet().doMute()
        self.dry().doMute()
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
