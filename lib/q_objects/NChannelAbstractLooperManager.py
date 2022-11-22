from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer, Qt
import re
import time
import os
import tempfile
import soundfile as sf

from ..StatesAndActions import *
from .LooperState import LooperState

# This looper manager manages a combination of multiple back-end loops
# which are hard-linked and represent audio channels on an abstract
# looper.
class NChannelAbstractLooperManager(LooperState):
    signalLoopAction = pyqtSignal(list, int, float) # loop idxs, action_id, arg
    loopIdxsChanged = pyqtSignal(list)
    loadLoopData = pyqtSignal(int, list) # loop idx, samples

    def __init__(self, parent=None, loop_idxs=[]):
        super(NChannelAbstractLooperManager, self).__init__(parent)
        self._loop_idxs = loop_idxs

    ######################
    # PROPERTIES
    ######################

    @pyqtProperty(list, notify=loopIdxsChanged)
    def loop_idxs(self):
        return self._loop_idxs
    @loop_idxs.setter
    def loop_idxs(self, l):
        if self._loop_idxs != l:
            self._loop_idxs = l
            self.loopIdxsChanged.emit(l)

    ##################
    # SLOTS
    ##################
    
    @pyqtSlot(int, list)
    def doLoopAction(self, action_id, arg):
        self.signalLoopAction.emit(self._loop_idxs, action_id, arg)

    @pyqtSlot(QObject)
    def connect_backend_manager(self, manager):
        if manager:
            self.signalLoopAction.connect(manager.do_loops_action)
            self.loadLoopData.connect(manager.load_loop_data)
            looper_state = manager.looper_states[self.loop_idxs[0]]
            looper_state.posChanged.connect(lambda v: NChannelAbstractLooperManager.pos.fset(self, v))
            looper_state.lengthChanged.connect(lambda v: NChannelAbstractLooperManager.length.fset(self, v))
            looper_state.stateChanged.connect(lambda v: NChannelAbstractLooperManager.state.fset(self, v))
            looper_state.nextStateChanged.connect(lambda v: NChannelAbstractLooperManager.next_state.fset(self, v))
            looper_state.volumeChanged.connect(lambda v: NChannelAbstractLooperManager.volume.fset(self, v))
            self.state = looper_state.state
            self.length = looper_state.length
            self.next_state = looper_state.next_state
            self.volume = looper_state.volume
            self.pos = looper_state.pos
            self.volumeChanged.connect(lambda v: self.doLoopAction(LoopActionType.SetLoopVolume, v))

    @pyqtSlot(result=str)
    def looper_type(self):
        return "NChannelAbstractLooperManager"
    
    # Load one array of samples into one channel
    @pyqtSlot(int, list)
    def load_loop_data(self, channel, data):
        self.loadLoopData.emit(self._loop_idxs[channel], data)
    
    @pyqtSlot(list)
    def load_loops_data(self, data):
        self.doLoopAction(LoopActionType.DoClear.value, [])
        for i in range(len(self._loop_idxs)):
            if i < len(data):
                # we got data for this channel
                self.load_loop_data(i, data[i])
            else:
                # we didn't get data for this channel
                print("Loaded sound data covers channels {}..{}, re-using first channel for channel {}".format(
                    1, len(data), i
                ))
                self.load_loop_data(i, data[0])
        if len(data) > len(self._loop_idxs):
            print("Discarding {} of loaded sound data's {} channels (have {} loop channels)".format(
                len(data) - len(self._loop_idxs),
                len(data),
                len(self._loop_idxs)
            ))