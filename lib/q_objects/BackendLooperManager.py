from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
import re
import time
import os
import wave
import tempfile

from ..LoopState import LoopState
from .LooperManager import LooperManager

# Looper manager for a back-end loop.
# Can be used to manage multiple back-end loops as well, in which case
# they should be hard-linked in the back-end. The manager will regard
# them as channels.
class BackendLooperManager(LooperManager):
    signalLoopAction = pyqtSignal(int, list) # action_id, args
    loopIdxsChanged = pyqtSignal(list)

    def __init__(self, parent=None, loop_idxs=[]):
        super(BackendLooperManager, self).__init__(parent)
        self._loop_idxs = loop_idxs
        # self.syncChanged.connect(self.update_sl_sync)
        # self.passthroughChanged.connect(self.update_sl_passthrough)
        # self.volumeChanged.connect(self.update_sl_volume)
        # self.panLChanged.connect(self.update_sl_pan_l)
        # self.panRChanged.connect(self.update_sl_pan_r)

        # self.stateChanged.connect(lambda s: print("State -> {}".format(s)))
        # self.lengthChanged.connect(lambda s: print("Length -> {}".format(s)))
        # self.posChanged.connect(lambda s: print("Position -> {}".format(s)))

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
            raise ValueError("BackendLooperManager with out-of-range loop idx")

        i = self._loop_idxs[0]
        self.length = lengths[i]
        self.pos = positions[i]
        self.volume = loop_volumes[i]

        # Front-end state extensions
        if self.length == 0:
            self.state = LoopState.Empty.value
        else:
            self.state = states[i]
    
    @pyqtSlot(int, list)
    def doLoopAction(self, action_id, args):
        self.signalLoopAction.emit(action_id, args)

    @pyqtSlot(QObject)
    def connect_backend_manager(self, manager):
        self.signalLoopAction.connect(
            lambda act, args: manager.do_loop_action(
                self.loop_idx,
                act,
                args
            )
        )
        manager.update.connect(self.update)
    
    @pyqtSlot(result=str)
    def looper_type(self):
        return "BackendLooperManager"
