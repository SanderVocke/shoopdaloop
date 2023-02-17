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

    # mode change notifications
    wetLooperChanged = pyqtSignal(QObject)
    dryLooperChanged = pyqtSignal(QObject)

    # To achieve certain recording/playback states, it is sometimes
    # necessary to override the dry or the wet looper's ports to have
    # passthrough. The pair manager requests this using these signals.
    forceWetPassthroughChanged = pyqtSignal(bool)
    forceDryPassthroughChanged = pyqtSignal(bool)

    # Additional signals which have special handling
    stopOtherLoopsInTrack = pyqtSignal()
    loadedData = pyqtSignal()
    loadMidiFile = pyqtSignal(str)
    saveMidiFile = pyqtSignal(str)

    didLoopAction = pyqtSignal(int, list, bool, bool) # action_id, args, with_soft_sync, propagate_to_selected_loops

    def __init__(self, dry_looper, wet_looper, parent=None):
        super(DryWetPairAbstractLooperManager, self).__init__(parent)
        self._parent = parent
        self._wet_looper = wet_looper
        self._dry_looper = dry_looper
        self._force_wet_passthrough = False
        self._force_dry_passthrough = False

        self.volumeChanged.connect(self.updateVolumes)

        # Connections
        for l in [self._wet_looper, self._dry_looper]:
            l.lengthChanged.connect(self.updateLength)
            l.posChanged.connect(self.updatePos)
            l.modeChanged.connect(self.updateState)
            l.nextModeChanged.connect(self.updateNextState)
            l.nextModeCountdownChanged.connect(self.updateNextStateCountdown)
            l.volumeChanged.connect(lambda v: DryWetPairAbstractLooperManager.volume.fset(self, v))
            l.outputPeakChanged.connect(lambda v: DryWetPairAbstractLooperManager.outputPeak.fset(self, v))
            l.loadedData.connect(lambda: self.loadedData.emit())
        self._dry_looper.cycled.connect(lambda: self.cycled.emit())
        self._dry_looper.passed_halfway.connect(lambda: self.passed_halfway.emit())
    
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
        if self.wet().mode == LoopMode.Stopped.value and \
            self.dry().mode == LoopMode.PlayingMuted.value:
            # Playing with live FX. show the position of the dry loop in this case
            self.position = self.dry().position
        else:
            self.position = self.wet().position
    
    @pyqtSlot()
    def updateState(self):
        # In most cases, our overall mode matches that of the wet loop.
        new_state = self.wet().mode

        if self.wet().mode == LoopMode.Recording.value and \
           self.dry().mode == LoopMode.Playing.value:
            new_state = LoopMode.RecordingFX.value
        
        if self.wet().mode == LoopMode.PlayingMuted.value and \
           self.dry().mode == LoopMode.Playing.value:
            new_state = LoopMode.PlayingLiveFX.value
        
        if self.wet().length <= 0.0 and self.dry().length <= 0.0:
            new_state = LoopMode.Empty.value
        
        if new_state != self.mode:
            self.mode = new_state
            self.modeChanged.emit(new_state)
    
    @pyqtSlot()
    def updateNextState(self):
        # In most cases, our overall mode matches that of the wet loop.
        new_next_mode = self.wet().next_mode

        if self.wet().next_mode == LoopMode.Recording.value and \
           self.dry().next_mode == LoopMode.Playing.value:
            new_next_mode = LoopMode.RecordingFX.value
        
        if self.wet().next_mode == LoopMode.PlayingMuted.value and \
           self.dry().next_mode == LoopMode.Playing.value:
            new_next_mode = LoopMode.PlayingLiveFX.value
        
        if self.wet().length <= 0.0 and self.dry().length <= 0.0 and \
           new_next_mode not in [LoopMode.Recording.value, LoopMode.RecordingFX.value]:
            new_next_mode = LoopMode.Empty.value
        
        if new_next_mode != self.next_mode:
            self.next_mode = new_next_mode
            self.nextModeChanged.emit(new_next_mode)
    
    @pyqtSlot()
    def updateNextStateCountdown(self):
        new_cd = self.wet().next_mode_countdown

        if new_cd != self.next_mode_countdown:
            self.next_mode_countdown = new_cd
            self.nextModeCountdownChanged.emit(new_cd)
    
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
    
    @pyqtSlot(int, list, bool, bool)
    def doLoopAction(self, action_id, args, with_soft_sync, propagate_to_selected_loops):
        if action_id == LoopActionType.DoPlaySoloInTrack.value:
            self.stopOtherLoopsInTrack.emit()
            action_id = LoopActionType.DoPlay.value
        if action_id == LoopActionType.DoTogglePlaying.value:
            action_id = (LoopActionType.DoStop.value if self.mode == LoopMode.Playing.value else LoopActionType.DoPlay.value)

        wet_action = action_id
        dry_action = action_id
        wet_args = args.copy()
        dry_args = args.copy()
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
                # For record N cycles, here we inject an additional argument
                # telling the dry/wet loopers what their next mode should be
                # after recording is finished.
                wet_action = LoopActionType.DoRecordNCycles.value
                dry_action = LoopActionType.DoPlay.value
                # Note that delay and N cycles should be correctly set from caller
                dry_args = [wet_args[0]]
                # TODO: this results in both loops still playing after the recording.
                # We need a DoPlayNCycles option!
                wet_args.append(float(LoopMode.Playing.value))
            case LoopActionType.DoRecordNCycles.value:
                # For record N cycles, here we inject an additional argument
                # telling the dry/wet loopers what their next mode should be
                # after recording is finished.
                wet_args.append(float(LoopMode.Playing.value))
                dry_args.append(float(LoopMode.PlayingMuted.value))
                force_dry_passthrough = True
        
        if force_dry_passthrough != None:
            self.force_dry_passthrough = force_dry_passthrough
        if force_wet_passthrough != None:
            self.force_wet_passthrough = force_wet_passthrough
        
        self.wet().doLoopAction(wet_action, wet_args, with_soft_sync, propagate_to_selected_loops)
        self.dry().doLoopAction(dry_action, dry_args, with_soft_sync, propagate_to_selected_loops)
        self.didLoopAction.emit(action_id, args, with_soft_sync, propagate_to_selected_loops)

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
    def doLoadMidiFile(self, filename):
        self.dry().load_midi_file(filename)
    
    @pyqtSlot(str)
    def doSaveWetToSoundFile(self, filename):
        self.doLoopAction(LoopActionType.DoStop.value, [0.0], False, False)
        self.wet().save_to_file(filename)
    
    @pyqtSlot(str)
    def doSaveDryToSoundFile(self, filename):
        self.doLoopAction(LoopActionType.DoStop.value, [0.0], False, False)
        self.dry().save_to_file(filename)
    
    @pyqtSlot(result=str)
    def looper_type(self):
        return "DryWetPairAbstractLooperManager"
    
    @pyqtSlot(int, int, int, result='QVariant')
    def get_waveforms(self, from_sample, to_sample, samples_per_bin):
        wet = self.wet_looper.get_waveforms(from_sample, to_sample, samples_per_bin)
        dry = self.dry_looper.get_waveforms(from_sample, to_sample, samples_per_bin)
        individual_results = [chan.get_waveforms(from_sample, to_sample, samples_per_bin) for chan in [self.wet_looper, self.dry_looper]]
        retval = {}
        for key in wet.keys():
            retval['wet_{}'.format(key)] = wet[key]
        for key in dry.keys():
            retval['dry_{}'.format(key)] = dry[key]
        return retval

    @pyqtSlot(result=list)
    def get_midi(self):
        return self.dry_looper.get_midi()
    
    @pyqtSlot(str)
    def doLoadMidiFile(self, filename):
        self.loadMidiFile.emit(filename)
    
    @pyqtSlot(str)
    def doSaveMidiFile(self, filename):
        self.saveMidiFile.emit(filename)
