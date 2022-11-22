from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer, Qt
import re
import time
import os
import tempfile

from ..StatesAndActions import *
from .PortState import PortState

# Wrap a port state with additional functionality to make port actions.
class PortManager(PortState):
    signalPortAction = pyqtSignal(int, int, float) # port idx, action, arg
    portIdxChanged = pyqtSignal(int)
    loadLoopData = pyqtSignal(int, list) # loop idx, samples

    def __init__(self, parent=None, port_idx=0):
        super(PortManager, self).__init__(parent)
        self._port_idx = port_idx

    ######################
    # PROPERTIES
    ######################

    @pyqtProperty(int, notify=portIdxChanged)
    def port_idx(self):
        return self._port_idx
    @port_idx.setter
    def port_idx(self, l):
        if self._port_idx != l:
            self._port_idx = l
            self.portIdxChanged.emit(l)

    ##################
    # SLOTS
    ##################
    
    @pyqtSlot(int)
    def doPortAction(self, action_id, arg):
        self.signalPortAction.emit(self._port_idx, action_id, arg)

    @pyqtSlot(QObject)
    def connect_backend_manager(self, manager):
        if manager:
            self.signalPortAction.connect(manager.do_port_action)
            port_state = manager.port_states[self.port_idx]
            port_state.volumeChanged.connect(lambda v: PortManager.volume.fset(self, v))
            port_state.mutedChanged.connect(lambda v: PortManager.muted.fset(self, v))
            port_state.passthroughChanged.connect(lambda v: PortManager.passthrough.fset(self, v))
            port_state.passthroughMutedChanged.connect(lambda v: PortManager.passthroughMuted.fset(self, v))
            self.volume = port_state.volume
            self.muted = port_state.muted
            self.passthrough = port_state.passthrough
            self.passthroughMuted = port_state.passthroughMuted
            self.volumeChanged.connect(lambda v: self.doPortAction(PortActionType.SetPortVolume, v))
            self.passthroughChanged.connect(lambda v: self.doPortAction(PortActionType.SetPortPassthrough, v))