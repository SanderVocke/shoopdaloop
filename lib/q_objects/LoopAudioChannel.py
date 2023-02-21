from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
from .AudioPort import AudioPort
import re
import time
import os
import tempfile
import json
from typing import *

import sys
sys.path.append('../..')

import lib.backend as backend
import lib.q_objects.Loop as Loop

# Wraps a back-end loop audio channel.
class LoopAudioChannel(QObject):
    # Other signals
    cycled = pyqtSignal()
    passed_halfway = pyqtSignal()

    def __init__(self, parent=None):
        super(LoopAudioChannel, self).__init__(parent)
        self._output_peak = 0.0
        self._volume = 0.0
        self._backend_obj = None        
        self._loop = None
        if parent and parent.inherits("Loop"):
            self._loop = parent
            self.maybe_initialize()
    
    def maybe_initialize(self):
        if self._loop and not self._backend_obj:
            self._backend_obj = self._loop.add_audio_channel(True)

    ######################
    # PROPERTIES
    ######################

    # loop
    loopChanged = pyqtSignal(Loop.Loop)
    @pyqtProperty(Loop.Loop, notify=loopChanged)
    def loop(self):
        return self._loop
    @loop.setter
    def loop(self, l):
        if l != self._loop:
            if self._loop or self._backend_obj:
                raise Exception('May not change loop of existing audio channel')
            self._loop = l
            self.maybe_initialize()

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

    @pyqtSlot(AudioPort)
    def connect(self, audio_port):
        if not self._backend_obj:
            raise Exception("Attempting to connect an invalid audio channel.")
        backend_channel = self._backend_obj
        backend_port = audio_port.get_backend_obj()
        backend_channel.connect(backend_port)
    
    @pyqtSlot(int, int, int, result=list)
    def get_rms_data(self, from_sample, to_sample, samples_per_bin):
        if not self._backend_obj:
            raise Exception("Attempting to get data of an invalid audio channel.")
        backend_channel = self._backend_obj
        return backend_channel.get_rms_data(from_sample, to_sample, samples_per_bin)
    
    @pyqtSlot(list)
    def load_data(self, data):
        if not self._backend_obj:
            raise Exception("Attempting to load data into an invalid audio channel.")
        backend_channel = self._backend_obj
        backend_channel.load_data(data)
    
    @pyqtSlot()
    def update(self):
        if not self._backend_obj:
            raise Exception("Attempting to update an invalid audio channel.")
        state = self._backend_obj.get_state()

        if state.output_peak != self._output_peak:
            self._output_peak = state.output_peak
            self.outputPeakChanged.emit(self._output_peak)
    
    @pyqtSlot()
    def close(self):
        self._backend_obj.destroy()
        self._backend_obj = None