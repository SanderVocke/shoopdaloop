import re
import time
import os
import tempfile
import json
import time
from typing import *
import sys

from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from .AudioPort import AudioPort
from .LoopChannel import LoopChannel

# Wraps a back-end loop audio channel.
class LoopAudioChannel(LoopChannel):

    def __init__(self, parent=None):
        super(LoopAudioChannel, self).__init__(parent)
        self._output_peak = 0.0
        self._volume = 0.0
    
    def maybe_initialize(self):
        if self._loop and self._loop.initialized and not self._backend_obj:
            self._backend_obj = self._loop.add_audio_channel(self.mode)
            self.initializedChanged.emit(True)

    ######################
    # PROPERTIES
    ######################

    # output peak
    outputPeakChanged = Signal(float)
    @Property(float, notify=outputPeakChanged)
    def output_peak(self):
        return self._output_peak
    
    # volume
    volumeChanged = Signal(float)
    @Property(float, notify=volumeChanged)
    def volume(self):
        return self._volume
    
    ######################
    # SLOTS
    ######################
    
    @Slot(int, int, int, result=list)
    def get_rms_data(self, from_sample, to_sample, samples_per_bin):
        if not self._backend_obj:
            raise Exception("Attempting to get data of an invalid audio channel.")
        backend_channel = self._backend_obj
        return backend_channel.get_rms_data(from_sample, to_sample, samples_per_bin)
    
    @Slot(list)
    def load_data(self, data):
        self.requestBackendInit.emit()
        if self._backend_obj:
            print("LOAD IMMEDIATELY {}".format(data))
            self._backend_obj.load_data(data)
        else:
            self.initializedChanged.connect(lambda: print("LOAD LATER {}".format(data)))
            self.initializedChanged.connect(lambda: self._backend_obj.load_data(data))
    
    @Slot(result=list)
    def get_data(self):
        if not self._backend_obj:
            raise Exception("Attempting to get data of an invalid audio channel.")
        return self._backend_obj.get_data()
    
    @Slot(float)
    def set_backend_volume(self, volume):
        if self._backend_obj:
            self._backend_obj.set_volume(volume)
    
    @Slot()
    def update_impl(self, state):
        if state.output_peak != self._output_peak:
            self._output_peak = state.output_peak
            self.outputPeakChanged.emit(self._output_peak)
        if state.volume != self._volume:
            self._volume = state.volume
            self.volumeChanged.emit(self._volume)