from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
import re
import time
import os
import tempfile
import json

import sys
sys.path.append('../..')

import lib.backend as backend

# Wraps a back-end loop audio channel.
class BackendLoopAudioChannel(QObject):
    # Other signals
    cycled = pyqtSignal()
    passed_halfway = pyqtSignal()

    def __init__(self, backend_obj : 'backend.BackendLoopAudioChannel', parent=None):
        super(BackendLoopAudioChannel, self).__init__(parent)
        self._output_peak = 0.0
        self._volume = 0.0
        self._backend_obj = backend_obj

    ######################
    # PROPERTIES
    ######################

    # output peak
    outputPeakChanged = pyqtSignal(float)
    @pyqtProperty(int, notify=outputPeakChanged)
    def output_peak(self):
        return self._output_peak
    @output_peak.setter
    def output_peak(self, s):
        if self._output_peak != s:
            self._output_peak = s
            self.outputPeakChanged.emit(s)
    
    # volume
    volumeChanged = pyqtSignal(float)
    @pyqtProperty(int, notify=volumeChanged)
    def volume(self):
        return self._volume
    @volume.setter
    def volume(self, s):
        if self._volume != s:
            self._volume = s
            self.volumeChanged.emit(s)
    
    ######################
    # SLOTS
    ######################

    @pyqtSlot(QObject) # BackendAudioPort
    def connect(self, audio_port):
        