from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer, Qt
import re
import time
import os
import tempfile

from ..StatesAndActions import *
from .PortManager import PortManager

def maximum_output_peak(port_managers):
    peaks = [p.outputPeak for p in port_managers]
    return max(peaks)

def maximum_input_peak(port_managers):
    peaks = [p.outputPeak for p in port_managers]
    return max(peaks)

def maximum_latency(port_managers):
    ls = [p.recordingLatency for p in port_managers]
    return max(ls)

# Wrap a port manager with additional functionality to make port actions.
class PortsManager(PortManager):
    def __init__(self, port_managers, parent=None):
        super(PortsManager, self).__init__(parent)
        self._port_managers = port_managers

        for p in self._port_managers:
            p.volumeChanged.connect(lambda v: PortManager.volume.fset(self, v))
            p.mutedChanged.connect(lambda v: PortManager.muted.fset(self, v))
            p.passthroughChanged.connect(lambda v: PortManager.passthrough.fset(self, v))
            p.passthroughMutedChanged.connect(lambda v: PortManager.passthroughMuted.fset(self, v))
            p.recordingLatencyChanged.connect(lambda v: PortManager.recordingLatency.fset(self, maximum_latency(self.port_managers)))
            p.outputPeakChanged.connect(lambda v: PortManager.outputPeak.fset(self, maximum_output_peak(self.port_managers)))
            p.inputPeakChanged.connect(lambda v: PortManager.inputPeak.fset(self, maximum_input_peak(self.port_managers)))
            self.volume = p.volume
            self.muted = p.muted
            self.passthrough = p.passthrough
            self.passthroughMuted = p.passthroughMuted
            self.recordingLatency = maximum_latency(self.port_managers)

    ######################
    # PROPERTIES
    ######################

    port_managersChanged = pyqtSignal(list)
    @pyqtProperty(list, notify=port_managersChanged)
    def port_managers(self):
        return self._port_managers
    
    ##################
    # SLOTS
    ##################
    
    @pyqtSlot(int)
    def doPortAction(self, action_id, arg):
        for p in self._port_managers:
            p.doPortAction(action_id, arg)