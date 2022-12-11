from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
import re
import time
import os
import tempfile
import json

# Represents the state of a port in the back-end
class PortState(QObject):

    # State change notifications
    volumeChanged = pyqtSignal(float)
    mutedChanged = pyqtSignal(bool)
    passthroughChanged = pyqtSignal(float)
    passthroughMutedChanged = pyqtSignal(bool)
    recordingLatencyChanged = pyqtSignal(float)
    outputPeakChanged = pyqtSignal(float)
    inputPeakChanged = pyqtSignal(float)

    def __init__(self, parent=None):
        super(PortState, self).__init__(parent)
        self._volume = 1.0
        self._muted = False
        self._passthrough = 1.0
        self._passthroughMuted = False
        self._recordingLatency = 0.0
        self._output_peak = 0.0
        self._input_peak = 0.0
    
    ######################
    # PROPERTIES
    ######################

    # volume
    @pyqtProperty(float, notify=volumeChanged)
    def volume(self):
        return self._volume
    @volume.setter
    def volume(self, l):
        if self._volume != l:
            self._volume = l
            self.volumeChanged.emit(l)
    
    # passthrough
    @pyqtProperty(float, notify=passthroughChanged)
    def passthrough(self):
        return self._passthrough
    @passthrough.setter
    def passthrough(self, l):
        if self._passthrough != l:
            self._passthrough = l
            self.passthroughChanged.emit(l)
    
    # muted
    @pyqtProperty(bool, notify=mutedChanged)
    def muted(self):
        return self._muted
    @muted.setter
    def muted(self, l):
        if self._muted != l:
            self._muted = l
            self.mutedChanged.emit(l)
    
    # passthroughMuted
    @pyqtProperty(bool, notify=passthroughMutedChanged)
    def passthroughMuted(self):
        return self._passthroughMuted
    @passthroughMuted.setter
    def passthroughMuted(self, l):
        if self._passthroughMuted != l:
            self._passthroughMuted = l
            self.passthroughMutedChanged.emit(l)
    
    # Output peak: amplitude value of 0.0-...
    @pyqtProperty(float, notify=outputPeakChanged)
    def outputPeak(self):
        return self._output_peak
    
    @outputPeak.setter
    def outputPeak(self, p):
        if self._output_peak != p:
            self._output_peak = p
            self.outputPeakChanged.emit(p)

    # Input peak: amplitude value of 0.0-...
    @pyqtProperty(float, notify=inputPeakChanged)
    def inputPeak(self):
        return self._input_peak
    
    @inputPeak.setter
    def inputPeak(self, p):
        if self._input_peak != p:
            self._input_peak = p
            self.inputPeakChanged.emit(p)
    
    @pyqtSlot(result=str)
    def serialize_session_state(self):
        d = {
            'volume' : self.volume,
            'passthrough' : self.passthrough,
            'muted' : self.muted,
            'passthroughMuted': self.passthroughMuted,
        }
        return json.dumps(d)
    
    @pyqtSlot(str)
    def deserialize_session_state(self, data):
        d = json.loads(data)
        self.volume = d['volume']
        self.passthrough = d['passthrough']
        self.muted = d['muted']
        self.passthroughMuted = d['passthroughMuted']
    
    # recording latency
    @pyqtProperty(float, notify=recordingLatencyChanged)
    def recordingLatency(self):
        return self._recordingLatency
    @recordingLatency.setter
    def recordingLatency(self, l):
        if self._recordingLatency != l:
            self._recordingLatency = l
            self.recordingLatencyChanged.emit(l)