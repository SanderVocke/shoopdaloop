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
from .ShoopPyObject import *

from ..logging import *

# Wraps a back-end loop audio channel.
class LoopAudioChannel(LoopChannel):
    def __init__(self, parent=None):
        super(LoopAudioChannel, self).__init__(parent)
        self._output_peak = self._new_output_peak = 0.0
        self._gain = self._new_gain = 1.0
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
    outputPeakChanged = ShoopSignal(float)
    @ShoopProperty(float, notify=outputPeakChanged)
    def output_peak(self):
        return self._output_peak
    
    # gain
    gainChanged = ShoopSignal(float)
    @ShoopProperty(float, notify=gainChanged)
    def gain(self):
        return self._gain
    
    ######################
    # SLOTS
    ######################
    
    @ShoopSlot(list, thread_protected=False)
    def load_data(self, data):
        self.requestBackendInit.emit()
        if self._backend_obj:
            self._backend_obj.load_data(data)
        else:
            self.initializedChanged.connect(lambda: self._backend_obj.load_data(data))
    
    @ShoopSlot(result=list, thread_protected=False)
    def get_data(self):
        if not self._backend_obj:
            self.logger.throw_error("Attempting to get data of an invalid audio channel.")
        return self._backend_obj.get_data()
    
    @ShoopSlot(float)
    def set_gain(self, gain):
        if self._backend_obj:
            self._backend_obj.set_gain(gain)
        else:
            self._gain = gain
            self.gainChanged.emit(gain)
    
    def updateOnOtherThreadSubclassImpl(self, state):
        if state.output_peak != self._new_output_peak:
            self._new_output_peak = state.output_peak
        else:
            if state.gain != self._new_gain:
                self._new_gain = state.gain
    
    def updateOnGuiThreadSubclassImpl(self):
        if self._output_peak != self._new_output_peak:
            self._output_peak = self._new_output_peak
            self.outputPeakChanged.emit(self._output_peak)
        else:
            if self._gain != self._new_gain:
                self._gain = self._new_gain
                self.gainChanged.emit(self._gain)