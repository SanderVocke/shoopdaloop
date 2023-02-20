from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
from .BackendAudioPort import BackendAudioPort
import re
import time
import os
import tempfile
import json
from typing import *

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
    @pyqtProperty(float, notify=outputPeakChanged)
    def output_peak(self):
        return self._output_peak
    
    # volume
    volumeChanged = pyqtSignal(float)
    @pyqtProperty(float, notify=volumeChanged)
    def volume(self):
        return self._volume
    
    ######################
    # SLOTS
    ######################

    @pyqtSlot(BackendAudioPort)
    def connect(self, audio_port):
        backend_channel = self._backend_obj
        backend_port = audio_port.get_backend_obj()
        backend_channel.connect(backend_port)
    
    @pyqtSlot(int, int, int, result=list)
    def get_rms_data(self, from_sample, to_sample, samples_per_bin):
        backend_channel = self._backend_obj
        return backend_channel.get_rms_data(from_sample, to_sample, samples_per_bin)
    
    @pyqtSlot(list)
    def load_data(self, data):
        backend_channel = self._backend_obj
        backend_channel.load_data(data)
    
    @pyqtSlot()
    def update(self):
        state = self._backend_obj.get_state()

        if state.output_peak != self._output_peak:
            self._output_peak = state.output_peak
            self.outputPeakChanged.emit(self._output_peak)
    
    @pyqtSlot()
    def close(self):
        self._backend_obj.destroy()
        self.destroy()