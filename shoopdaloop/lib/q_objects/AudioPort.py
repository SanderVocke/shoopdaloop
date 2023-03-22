import re
import time
import os
import tempfile
import json
from typing import *
import sys

from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
from PyQt6.QtQuick import QQuickItem

from .Port import Port

# Wraps a back-end port.
class AudioPort(Port):
    def __init__(self, parent=None):
        super(AudioPort, self).__init__(parent)
        self._peak = 0.0
        self._volume = 1.0
        self._passthrough_volume = 1.0

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
    
    # passthrough volume
    passthroughVolumeChanged = pyqtSignal(float)
    @pyqtProperty(float, notify=passthroughVolumeChanged)
    def passthrough_volume(self):
        return self._passthrough_volume
    @passthrough_volume.setter
    def passthrough_volume(self, s):
        if self._passthrough_volume != s:
            self._passthrough_volume = s
            self.passthroughVolumeChanged.emit(s)
    
    ###########
    ## SLOTS
    ###########

    # Update mode from the back-end.
    @pyqtSlot()
    def update(self):
        if not self.initialized:
            return
        state = self._backend_obj.get_state()
        self.peak = state.peak
        self.volume = state.volume
        self.passthrough_volume = state.passthrough_volume
        self.name = state.name
        self.muted = state.muted
        self.passthrough_muted = state.muted
    
    @pyqtSlot(float)
    def set_backend_volume(self, volume):
        if self._backend_obj:
            self._backend_obj.set_volume(volume)
    
    @pyqtSlot(float)
    def set_backend_passthrough_volume(self, passthrough_volume):
        if self._backend_obj:
            self._backend_obj.set_passthrough_volume(passthrough_volume)

    ##########
    ## INTERNAL MEMBERS
    ##########
    def get_backend_obj(self):
        return self._backend_obj
        
    def maybe_initialize_impl(self, name_hint, direction):
        self._backend_obj = self.backend.get_backend_obj().open_audio_port(name_hint, direction)

    def connect_passthrough_impl(self, other):
        self._backend_obj.connect_passthrough(other.get_backend_obj())