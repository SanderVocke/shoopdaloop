from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer, Qt
import re
import time
import os
import wave
import tempfile

from ..LoopState import LoopState
from .BasicLooperManager import BasicLooperManager

# This looper manager manages a combination of multiple back-end loops
# which are hard-linked and represent audio channels on an abstract
# looper.
class NChannelAbstractLooperManager(BasicLooperManager):
    signalLoopAction = pyqtSignal(int, list) # action_id, args
    loopIdxsChanged = pyqtSignal(list)

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

    @pyqtSlot(
        int,
        int,
        list,
        list,
        list,
        list,
        list,
        list,
        list)
    def update(
        self,
        n_loops,
        n_ports,
        states,
        next_states,
        lengths,
        positions,
        loop_volumes,
        port_volumes,
        port_passthroughs
    ):
        # We just use updates of our first managed loop because
        # all loops should be hard-synced anyway.
        if self._loop_idxs[0] >= n_loops:
            raise ValueError("NChannelAbstractLooperManager with out-of-range loop idx")

        i = self._loop_idxs[0]
        self.length = lengths[i]
        self.pos = positions[i]
        self.volume = loop_volumes[i]

        # Front-end state extensions
        if self.length == 0:
            self.state = LoopState.Empty.value
        else:
            self.state = states[i]
        if self.state == LoopState.Empty.value and \
           next_states[i] in [
            LoopState.Playing.value,
            LoopState.PlayingMuted.value,
            LoopState.Stopped.value,
            LoopState.Unknown.value
           ]:
            self.next_state = LoopState.Empty.value
        else:
            self.next_state = next_states[i]
    
    @pyqtSlot(int, list)
    def doLoopAction(self, action_id, args):
        self.signalLoopAction.emit(action_id, args)

    @pyqtSlot(QObject)
    def connect_backend_manager(self, manager):
        if manager:
            self.signalLoopAction.connect(
                lambda act, args: manager.do_loop_action(
                    self.loop_idxs[0],
                    act,
                    args
                )
            )
            manager.looper_mgrs[self.loop_idxs[0]].posChanged.connect(lambda v: NChannelAbstractLooperManager.pos.fset(self, v))
            manager.looper_mgrs[self.loop_idxs[0]].lengthChanged.connect(lambda v: NChannelAbstractLooperManager.length.fset(self, v))
            manager.looper_mgrs[self.loop_idxs[0]].stateChanged.connect(lambda v: NChannelAbstractLooperManager.state.fset(self, v))
            manager.looper_mgrs[self.loop_idxs[0]].nextStateChanged.connect(lambda v: NChannelAbstractLooperManager.next_state.fset(self, v))
            manager.looper_mgrs[self.loop_idxs[0]].volumeChanged.connect(lambda v: NChannelAbstractLooperManager.volume.fset(self, v))

    @pyqtSlot(result=str)
    def looper_type(self):
        return "NChannelAbstractLooperManager"
