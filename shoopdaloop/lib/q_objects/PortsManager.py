import re
import time
import os
import tempfile

from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer, Qt

from .PortManager import PortManager

from ..StatesAndActions import *

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
            p.volumeChanged.connect(lambda v: self.set_volume_direct(v))
            p.mutedChanged.connect(lambda v: self.set_muted_direct(v))
            p.passthroughChanged.connect(lambda v: self.set_passthrough_direct(v))
            p.passthroughMutedChanged.connect(lambda v: self.set_passthroughMuted_direct(v))
            p.recordingLatencyChanged.connect(lambda v: self.set_recordingLatency_direct(maximum_latency(self.port_managers)))
            p.outputPeakChanged.connect(lambda v: PortManager.outputPeak.fset(self, maximum_output_peak(self.port_managers)))
            p.inputPeakChanged.connect(lambda v: PortManager.inputPeak.fset(self, maximum_input_peak(self.port_managers)))
            self.volume = p.volume
            self.muted = p.muted
            self.passthrough = p.passthrough
            self.passthroughMuted = p.passthroughMuted
            self.set_recordingLatency_direct(maximum_latency(self.port_managers))

    ######################
    # PROPERTIES
    ######################

    port_managersChanged = Signal(list)
    @Property(list, notify=port_managersChanged)
    def port_managers(self):
        return self._port_managers
    
    # Override setters to redirect them to our "children".
    # We will not update our own property - we expect that to happen
    # after a round-trip to the back-end.
    volumeChanged = Signal(float)
    @Property(float, notify=volumeChanged)
    def volume(self):
        return self._volume
    @volume.setter
    def volume(self, v):
        for p in self._port_managers:
            p.volume = v
    def set_volume_direct(self, v):
        if v != self._volume:
            self._volume = v
            self.volumeChanged.emit(v)
    
    # passthrough
    passthroughChanged = Signal(float)
    @Property(float, notify=passthroughChanged)
    def passthrough(self):
        return self._passthrough
    @passthrough.setter
    def passthrough(self, v):
        for p in self._port_managers:
            p.passthrough = v
    def set_passthrough_direct(self, l):
        if self._passthrough != l:
            self._passthrough = l
            self.passthroughChanged.emit(l)
    
    # muted
    mutedChanged = Signal(bool)
    @Property(bool, notify=mutedChanged)
    def muted(self):
        return self._muted
    @muted.setter
    def muted(self, v):
        for p in self._port_managers:
            p.muted = v
    def set_muted_direct(self, l):
        if self._muted != l:
            self._muted = l
            self.mutedChanged.emit(l)
    
    # passthroughMuted
    passthroughMutedChanged = Signal(bool)
    @Property(bool, notify=passthroughMutedChanged)
    def passthroughMuted(self):
        return self._passthroughMuted
    @passthroughMuted.setter
    def passthroughMuted(self, v):
        for p in self._port_managers:
            p.passthroughMuted = v
    def set_passthroughMuted_direct(self, l):
        if self._passthroughMuted != l:
            self._passthroughMuted = l
            self.passthroughMutedChanged.emit(l)
    
    # recording latency
    recordingLatencyChanged = Signal(float)
    @Property(float, notify=recordingLatencyChanged)
    def recordingLatency(self):
        return self._recordingLatency
    def set_recordingLatency_direct(self, l):
        if self._recordingLatency != l:
            self._recordingLatency = l
            self.recordingLatencyChanged.emit(l)
    
    ##################
    # SLOTS
    ##################
    
    @Slot(int)
    def doPortAction(self, action_id, arg):
        for p in self._port_managers:
            p.doPortAction(action_id, arg)