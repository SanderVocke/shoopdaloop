from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer, Qt
import re
import time
import os
import tempfile
import soundfile as sf

from ..StatesAndActions import *
from .LooperState import LooperState

def maximum_output_peak(loopers):
    ps = [l.outputPeak for l in loopers]
    return max(ps)

# This looper manager manages a combination of multiple back-end loops
# which are hard-linked and represent audio channels on an abstract
# looper.
class NChannelAbstractLooperManager(LooperState):
    signalLoopAction = pyqtSignal(int, list, bool, bool) # action_id, args, with_soft_sync, propagate_to_selected_loops
    saveToFile = pyqtSignal(str)
    loadFromFile = pyqtSignal(str)
    loadedData = pyqtSignal()

    def __init__(self, channel_loopers, parent=None):
        super(NChannelAbstractLooperManager, self).__init__(parent)
        self._channel_loopers = channel_loopers

        # Sync our own mode from the first looper.
        first_looper = self._channel_loopers[0]
        first_looper.posChanged.connect(lambda v: NChannelAbstractLooperManager.pos.fset(self, v))
        first_looper.lengthChanged.connect(lambda v: NChannelAbstractLooperManager.length.fset(self, v))
        first_looper.modeChanged.connect(lambda v: NChannelAbstractLooperManager.mode.fset(self, v))
        first_looper.nextModeChanged.connect(lambda v: NChannelAbstractLooperManager.next_mode.fset(self, v))
        first_looper.nextModeCountdownChanged.connect(lambda v: NChannelAbstractLooperManager.next_mode_countdown.fset(self, v))
        first_looper.volumeChanged.connect(lambda v: NChannelAbstractLooperManager.volume.fset(self, v))
        first_looper.outputPeakChanged.connect(lambda v: NChannelAbstractLooperManager.outputPeak.fset(self, maximum_output_peak(self.channel_loopers)))
        for l in self._channel_loopers:
            
            self.mode = first_looper.mode
            self.length = first_looper.length
            self.next_mode = first_looper.next_mode
            self.next_mode_countdown = first_looper.next_mode_countdown
            self.volume = first_looper.volume
            self.pos = first_looper.pos
            self.outputPeak = first_looper.outputPeak
        
        self.volumeChanged.connect(lambda v: self.doLoopAction(LoopActionType.SetLoopVolume.value, [v], True, False))
        self._channel_loopers[0].cycled.connect(lambda: self.cycled.emit())
        self._channel_loopers[0].passed_halfway.connect(lambda: self.passed_halfway.emit())

    ######################
    # PROPERTIES
    ######################
    
    channel_loopersChanged = pyqtSignal(list)
    @pyqtProperty(list, notify=channel_loopersChanged)
    def channel_loopers(self):
        return self._channel_loopers

    ##################
    # SLOTS
    ##################
    
    @pyqtSlot(int, list, bool, bool)
    def doLoopAction(self, action_id, args, with_soft_sync, propagate_to_selected_loops):
        self.signalLoopAction.emit(action_id, args, with_soft_sync, propagate_to_selected_loops)

    @pyqtSlot(result=str)
    def looper_type(self):
        return "NChannelAbstractLooperManager"
    
    # Save into a file.
    @pyqtSlot(str)
    def save_to_file(self, filename):
        self.saveToFile.emit(filename)
    
    # Load from a file.
    @pyqtSlot(str)
    def load_from_file(self, filename):
        self.loadFromFile.emit(filename)
    
    @pyqtSlot(int, int, int, result='QVariant')
    def get_waveforms(self, from_sample, to_sample, samples_per_bin):
        channel_results = [chan.get_waveforms(from_sample, to_sample, samples_per_bin) for chan in self._channel_loopers]
        retval = {}
        for idx, r in enumerate(channel_results):
            for key in r.keys():
                # rename the keys
                retval['chan_{}_{}'.format(idx, key)] = r[key]
        return retval
    
    @pyqtSlot(result=list)
    def get_midi(self):
        return self._channel_loopers[0].get_midi()
        
            