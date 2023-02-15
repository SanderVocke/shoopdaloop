from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
import re
import time
import os
import tempfile
import json

from ..StatesAndActions import PortActionType

# Represents the state of a port in the back-end
class PortManager(QObject):

    # state change notifications
    volumeChanged = pyqtSignal(float)
    mutedChanged = pyqtSignal(bool)
    passthroughChanged = pyqtSignal(float)
    passthroughMutedChanged = pyqtSignal(bool)
    recordingLatencyChanged = pyqtSignal(float)
    outputPeakChanged = pyqtSignal(float)
    inputPeakChanged = pyqtSignal(float)
    signalPortAction = pyqtSignal(int, float) # action, arg

    def __init__(self, parent=None):
        super(PortManager, self).__init__(parent)
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
    def volume(self, v):
        # Let back-end update the property for us
        self.signalPortAction.emit(PortActionType.SetPortVolume.value, v)
    # To modify from the back-end
    def set_volume_direct(self, v):
        if v != self._volume:
            self._volume = v
            self.volumeChanged.emit(v)
    
    # passthrough
    @pyqtProperty(float, notify=passthroughChanged)
    def passthrough(self):
        return self._passthrough
    @passthrough.setter
    def passthrough(self, v):
        # Let back-end update the property for us
        self.signalPortAction.emit(PortActionType.SetPortPassthrough.value, v)
    def set_passthrough_direct(self, l):
        if self._passthrough != l:
            self._passthrough = l
            self.passthroughChanged.emit(l)
    
    # muted
    @pyqtProperty(bool, notify=mutedChanged)
    def muted(self):
        return self._muted
    @muted.setter
    def muted(self, v):
        # Let back-end update the property for us
        self.signalPortAction.emit(PortActionType.DoMute.value if v else PortActionType.DoUnmute.value, v)
    def set_muted_direct(self, l):
        if self._muted != l:
            self._muted = l
            self.mutedChanged.emit(l)
    
    # passthroughMuted
    @pyqtProperty(bool, notify=passthroughMutedChanged)
    def passthroughMuted(self):
        return self._passthroughMuted
    @passthroughMuted.setter
    def passthroughMuted(self, v):
        # Let back-end update the property for us
        self.signalPortAction.emit(PortActionType.DoMuteInput.value if v else PortActionType.DoUnmuteInput.value, v)
    def set_passthroughMuted_direct(self, l):
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
    def set_recordingLatency_direct(self, l):
        if self._recordingLatency != l:
            self._recordingLatency = l
            self.recordingLatencyChanged.emit(l)
    
    ###############
    ## SLOTS
    ###############

    @pyqtSlot(int)
    def doPortAction(self, action_id, arg):
        self.signalPortAction.emit(action_id, arg)