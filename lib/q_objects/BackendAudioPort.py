from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
import re
import time
import os
import tempfile
import json
from typing import *

import sys
sys.path.append('../..')

import lib.backend as backend

# Wraps a back-end port.
class BackendAudioPort(QObject):
    def __init__(self, backend_port : Type[backend.BackendAudioPort], parent=None):
        super(BackendAudioPort, self).__init__(parent)
        self._peak = 0.0
        self._volume = 1.0
        self._name = ''
        self._backend_obj = backend_port

    ######################
    # PROPERTIES
    ######################

    # peak
    peakChanged = pyqtSignal(float)
    @pyqtProperty(float, notify=peakChanged)
    def peak(self):
        return self._peak
    @peak.setter
    def peak(self, s):
        if self._peak != s:
            self._peak = s
            self.peakChanged.emit(s)
    
    # volume
    volumeChanged = pyqtSignal(float)
    @pyqtProperty(float, notify=volumeChanged)
    def volume(self):
        return self._volume
    @volume.setter
    def volume(self, s):
        if self._volume != s:
            self._volume = s
            self.volumeChanged.emit(s)
    
    # name
    nameChanged = pyqtSignal(str)
    @pyqtProperty(str, notify=nameChanged)
    def name(self):
        return self._name
    @name.setter
    def volume(self, s):
        if self._name != s:
            self._name = s
            self.nameChanged.emit(s)
    
    ###########
    ## SLOTS
    ###########

    # Update mode from the back-end.
    @pyqtSlot()
    def update(self):
        state = self._backend_obj.get_state()
        self.peak = state.peak
        self.volume = state.volume
        self.name = state.name
    
    # Get the wrapped back-end object.
    @pyqtSlot(result=backend.BackendAudioPort)
    def get_backend_obj(self):
        return self._backend_obj