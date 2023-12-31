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

from ..logging import *

# Wraps a back-end loop audio channel.
class LoopAudioChannel(LoopChannel):

    def __init__(self, parent=None):
        super(LoopAudioChannel, self).__init__(parent)
        self._output_peak = 0.0
        self._gain = 1.0
        self._initial_gain_pushed = False
        self.logger = Logger("Frontend.AudioChannel")
    
    def maybe_initialize(self):
        if self._loop and self._loop.initialized and not self._backend_obj:
            self._backend_obj = self._loop.add_audio_channel(self.mode)
            self.logger.debug(lambda: "Initialized back-end channel")
            self.initializedChanged.emit(True)
            self.set_gain(self._gain)
    
    ######################
    # PROPERTIES
    ######################

    # output peak
    outputPeakChanged = Signal(float)
    @Property(float, notify=outputPeakChanged)
    def output_peak(self):
        return self._output_peak
    
    # gain
    gainChanged = Signal(float)
    @Property(float, notify=gainChanged)
    def gain(self):
        return self._gain
    
    ######################
    # SLOTS
    ######################
    
    @Slot(list)
    def load_data(self, data):
        self.requestBackendInit.emit()
        if self._backend_obj:
            self._backend_obj.load_data(data)
        else:
            self.initializedChanged.connect(lambda: self._backend_obj.load_data(data))
    
    @Slot(result=list)
    def get_data(self):
        if not self._backend_obj:
            self.logger.throw_error("Attempting to get data of an invalid audio channel.")
        return self._backend_obj.get_data()
    
    @Slot(float)
    def set_gain(self, gain):
        if self._backend_obj:
            self._backend_obj.set_gain(gain)
        else:
            self._gain = gain
            self.gainChanged.emit(gain)
    
    @Slot()
    def update_impl(self, state):
        if state.output_peak != self._output_peak:
            self._output_peak = state.output_peak
            self.outputPeakChanged.emit(self._output_peak)
        else:
            if state.gain != self._gain:
                self._gain = state.gain
                self.gainChanged.emit(self._gain)