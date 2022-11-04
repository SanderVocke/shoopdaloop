from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
import re
import time
import os
import wave
import tempfile

from ..LoopState import LoopState
from .LooperManager import LooperManager

# Looper manager for a single loop in the back-end.
class BackendLooperManager(LooperManager):
    signalLoopAction = pyqtSignal(int, list) # action_id, args
    loopIdxChanged = pyqtSignal(int)

    def __init__(self, parent=None, loop_idx=0):
        super(BackendLooperManager, self).__init__(parent)
        self._loop_idx = loop_idx
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

    @pyqtProperty(int, notify=loopIdxChanged)
    def loop_idx(self):
        return self._loop_idx
    @loop_idx.setter
    def loop_idx(self, l):
        if self._loop_idx != l:
            self._loop_idx = l
            self.loopIdxChanged.emit(l)

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
        if self._loop_idx >= n_loops:
            raise ValueError("BackendLooperManager with out-of-range loop idx")

        i = self._loop_idx
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
