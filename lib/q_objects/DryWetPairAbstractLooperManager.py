from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
import re
import time
import os
import tempfile
from enum import Enum
import math
from ..StatesAndActions import *
from .NChannelAbstractLooperManager import NChannelAbstractLooperManager
from .LooperState import LooperState

# Combines two loops into a single looper interface which
# offers an FX send/return by using one loop for the dry and one for the
# wet signal.
class DryWetPairAbstractLooperManager(LooperState):

    # State change notifications
    wetLooperChanged = pyqtSignal(QObject)
    dryLooperChanged = pyqtSignal(QObject)

    # To achieve certain recording/playback states, it is sometimes
    # necessary to override the dry or the wet looper's ports to have
    # passthrough. The pair manager requests this using these signals.
    forceWetPassthroughChanged = pyqtSignal(bool)
    forceDryPassthroughChanged = pyqtSignal(bool)

    def __init__(self, dry_looper, wet_looper, parent=None):
        super(DryWetPairAbstractLooperManager, self).__init__(parent)
        self._parent = parent
        self._wet_looper = wet_looper
        self._dry_looper = dry_looper
        self._force_wet_passthrough = False
        self._force_dry_passthrough = False

        self.volumeChanged.connect(self.updateVolumes)
        # self.panLChanged.connect(self.updatePans)
        # self.panRChanged.connect(self.updatePans)

        # Connections
        for l in [self._wet_looper, self._dry_looper]:
            l.lengthChanged.connect(self.updateLength)
            l.posChanged.connect(self.updatePos)
            l.stateChanged.connect(self.updateState)
            l.nextStateChanged.connect(self.updateNextState)
            l.nextStateCountdownChanged.connect(self.updateNextStateCountdown)
            l.volumeChanged.connect(lambda v: DryWetPairAbstractLooperManager.volume.fset(self, v))
            l.outputPeakChanged.connect(lambda v: DryWetPairAbstractLooperManager.outputPeak.fset(self, v))
    
    ######################
    # PROPERTIES
    ######################
    
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
        return self._wet_looper
    
    def dry(self):
        return self._dry_looper

    @pyqtSlot()
    def updateLength(self):
        self.length = max(self.wet().length, self.dry().length)
        self.updateState()
        self.updateNextState()
        self.updateNextStateCountdown()
    
    @pyqtSlot()
    def updatePos(self):
        # In most cases, we want to show the position of the wet
        # loop.        
        if self.wet().state == LoopState.Stopped.value and \
            self.dry().state == LoopState.PlayingMuted.value:
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
        
        if self.wet().state == LoopState.PlayingMuted.value and \
           self.dry().state == LoopState.Playing.value:
            new_state = LoopState.PlayingLiveFX.value
        
        if self.wet().length <= 0.0 and self.dry().length <= 0.0:
            new_state = LoopState.Empty.value
        
        if new_state != self.state:
            self.state = new_state
            self.stateChanged.emit(new_state)
    
    @pyqtSlot()
    def updateNextState(self):
        # In most cases, our overall state matches that of the wet loop.
        new_next_state = self.wet().next_state

        if self.wet().next_state == LoopState.Recording.value and \
           self.dry().next_state == LoopState.Playing.value:
            new_next_state = LoopState.RecordingFX.value
        
        if self.wet().next_state == LoopState.PlayingMuted.value and \
           self.dry().next_state == LoopState.Playing.value:
            new_next_state = LoopState.PlayingLiveFX.value
        
        if self.wet().length <= 0.0 and self.dry().length <= 0.0 and \
           new_next_state not in [LoopState.Recording.value, LoopState.RecordingFX.value]:
            new_next_state = LoopState.Empty.value
        
        if new_next_state != self.next_state:
            self.next_state = new_next_state
            self.nextStateChanged.emit(new_next_state)
    
    @pyqtSlot()
    def updateNextStateCountdown(self):
        new_cd = self.wet().next_state_countdown

        if new_cd != self.next_state_countdown:
            self.next_state_countdown = new_cd
            self.nextStateCountdownChanged.emit(new_cd)
    
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
    
    @pyqtSlot(int, list, bool)
    def doLoopAction(self, action_id, args, with_soft_sync):
        wet_action = action_id
        dry_action = action_id
        wet_args = args
        dry_args = args
        force_dry_passthrough = False
        force_wet_passthrough = False

        match action_id:
            case LoopActionType.DoPlay.value:
                wet_action = LoopActionType.DoPlay.value
                dry_action = LoopActionType.DoPlayMuted.value
            case LoopActionType.DoPlayLiveFX.value:
                wet_action = LoopActionType.DoPlayMuted.value
                dry_action = LoopActionType.DoPlay.value
                force_wet_passthrough = True
            case LoopActionType.DoRecord.value:
                force_dry_passthrough = True
            case LoopActionType.DoReRecordFX.value:
                wet_action = LoopActionType.DoRecordNCycles.value
                dry_action = LoopActionType.DoPlay.value
                # Note that delay and N cycles should be correctly set from caller
                dry_args = [wet_args[0]]
            case LoopActionType.DoRecordNCycles.value:
                force_dry_passthrough = True
        
        self.wet().doLoopAction(wet_action, wet_args, with_soft_sync)
        self.dry().doLoopAction(dry_action, dry_args, with_soft_sync)
        if force_dry_passthrough != None:
            self.force_dry_passthrough = force_dry_passthrough
        if force_wet_passthrough != None:
            self.force_wet_passthrough = force_wet_passthrough

    @pyqtSlot(str)
    def doLoadWetSoundFile(self, filename):
        self.wet().load_from_file(filename)
    
    @pyqtSlot(str)
    def doLoadDrySoundFile(self, filename):
        self.dry().load_from_file(filename)

    @pyqtSlot(str)
    def doLoadSoundFile(self, filename):
        self.doLoadDrySoundFile(filename)
        self.doLoadWetSoundFile(filename)
    
    @pyqtSlot(str)
    def doSaveWetToSoundFile(self, filename):
        self.doLoopAction(LoopActionType.DoStop.value, [0.0], False)
        self.wet().save_to_file(filename)
    
    @pyqtSlot(str)
    def doSaveDryToSoundFile(self, filename):
        self.doLoopAction(LoopActionType.DoStop.value, [0.0], False)
        self.dry().save_to_file(filename)
    
    @pyqtSlot(result=str)
    def looper_type(self):
        return "DryWetPairAbstractLooperManager"
