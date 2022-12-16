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
    signalLoopAction = pyqtSignal(int, list, bool) # action_id, args, with_soft_sync
    loadLoopData = pyqtSignal(int, list) # channel idx, samples
    saveToFile = pyqtSignal(str)
    loadFromFile = pyqtSignal(str)

    def __init__(self, channel_loopers, parent=None):
        super(NChannelAbstractLooperManager, self).__init__(parent)
        self._channel_loopers = channel_loopers

        # Sync our own state from the first looper.
        for l in self._channel_loopers:
            first_looper = self._channel_loopers[0]
            first_looper.posChanged.connect(lambda v: NChannelAbstractLooperManager.pos.fset(self, v))
            first_looper.lengthChanged.connect(lambda v: NChannelAbstractLooperManager.length.fset(self, v))
            first_looper.stateChanged.connect(lambda v: NChannelAbstractLooperManager.state.fset(self, v))
            first_looper.nextStateChanged.connect(lambda v: NChannelAbstractLooperManager.next_state.fset(self, v))
            first_looper.nextStateCountdownChanged.connect(lambda v: NChannelAbstractLooperManager.next_state_countdown.fset(self, v))
            first_looper.volumeChanged.connect(lambda v: NChannelAbstractLooperManager.volume.fset(self, v))
            first_looper.outputPeakChanged.connect(lambda v: NChannelAbstractLooperManager.outputPeak.fset(self, maximum_output_peak(self.channel_loopers)))
            self.state = first_looper.state
            self.length = first_looper.length
            self.next_state = first_looper.next_state
            self.next_state_countdown = first_looper.next_state_countdown
            self.volume = first_looper.volume
            self.pos = first_looper.pos
            self.outputPeak = first_looper.outputPeak
        
        self.volumeChanged.connect(lambda v: self.doLoopAction(LoopActionType.SetLoopVolume.value, v, True))

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
    
    @pyqtSlot(int, list, bool)
    def doLoopAction(self, action_id, args, with_soft_sync):
        self.signalLoopAction.emit(action_id, args, with_soft_sync)

    @pyqtSlot(result=str)
    def looper_type(self):
        return "NChannelAbstractLooperManager"
    
    # Load one array of samples into one channel
    @pyqtSlot(int, list)
    def load_loop_data(self, channel, data):
        self.loadLoopData.emit(channel, data)
    
    # Save into a file.
    @pyqtSlot(str)
    def save_to_file(self, filename):
        self.saveToFile.emit(filename)
    
    # Load from a file.
    @pyqtSlot(str)
    def load_from_file(self, filename):
        self.loadFromFile.emit(filename)
    
    @pyqtSlot(list)
    def load_loops_data(self, data):
        self.doLoopAction(LoopActionType.DoClear.value, 0.0, False)
        for i in range(len(self._channel_loopers)):
            if i < len(data):
                # we got data for this channel
                self.load_loop_data(i, data[i])
            else:
                # we didn't get data for this channel
                print("Loaded sound data covers channels {}..{}, re-using first channel for channel {}".format(
                    1, len(data), i
                ))
                self.load_loop_data(i, data[0])
        if len(data) > len(self._channel_loopers):
            print("Discarding {} of loaded sound data's {} channels (have {} loop channels)".format(
                len(data) - len(self._channel_loopers),
                len(data),
                len(self._channel_loopers)
            ))