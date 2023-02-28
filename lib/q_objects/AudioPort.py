from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
from PyQt6.QtQuick import QQuickItem
from .Port import Port
import re
import time
import os
import tempfile
import json
from typing import *

import sys
sys.path.append('../..')

import lib.backend_wrappers as backend

# Wraps a back-end port.
class AudioPort(Port):
    def __init__(self, parent=None):
        super(AudioPort, self).__init__(parent)
        self._peak = 0.0
        self._volume = 1.0

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
    
    ##########
    ## INTERNAL MEMBERS
    ##########
    def maybe_initialize_impl(self, name_hint, direction):
        self._backend_obj = backend.open_audio_port(name_hint, direction)
