from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
import re
import time
import os
import wave
import tempfile
from enum import Enum
import math

from ..LoopState import *
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
                raise Exception("Changing loop idx of already existing looper.")
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
    def updateVolumes(self):
        self.wet().volume = self.volume
        self.dry().volume = 1.0
    
    @pyqtSlot()
    def updatePans(self):
        self.dry().panL = self.panL
        self.dry().panR = self.panR
        self.wet().panL = 0.0
        self.wet().panR = 1.0
    
    @pyqtSlot(int, list)
    def doLoopAction(self, action_id, args):
        wet_action = action_id
        dry_action = action_id
        wet_args = args
        dry_args = args
        force_dry_passthrough = False
        force_wet_passthrough = False

        match action_id:
            case LoopActionType.DoPlay.value:
                wet_action = LoopActionType.DoPlay.value
                dry_action = LoopActionType.DoStop.value
            case LoopActionType.DoPlayLiveFX.value:
                wet_action = LoopActionType.DoStop.value
                dry_action = LoopActionType.DoPlay.value
                force_wet_passthrough = True
            case LoopActionType.DoRecord.value:
                force_dry_passthrough = True
            case LoopActionType.DoRecordFX.value:
                # TODO: N cycles
                wet_action = LoopActionType.DoRecord.value
                dry_action = LoopActionType.DoPlay.value
            case LoopActionType.doRecordNCycles.value:
                force_dry_passthrough = True
        
        self.wet().doLoopAction(wet_action, wet_args)
        self.dry().doLoopAction(dry_action, dry_args)
        if force_dry_passthrough != None:
            self.force_dry_passthrough = force_dry_passthrough
        if force_wet_passthrough != None:
            self.force_wet_passthrough = force_wet_passthrough

    @pyqtSlot(str)
    def doLoadWav(self, wav_file):
        # Load the wav into both dry and wet loops.
        # The user can always adjust later.
        self.dry().doLoadWav(wav_file)
        self.wet().doLoadWav(wav_file)

    @pyqtSlot(str)
    def doSaveWav(self, wav_file):
        raise NotImplementedError()
    
    @pyqtSlot(QObject)
    def connect_backend_manager(self, manager):
        self.wet().connect_backend_manager(manager)
        self.dry().connect_backend_manager(manager)
    
    @pyqtSlot(result=str)
    def looper_type(self):
        return "BackendFXLooperPairManager"
