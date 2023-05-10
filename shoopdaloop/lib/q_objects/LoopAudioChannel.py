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
        self._volume = 1.0
        self._initial_volume_pushed = False
    
    def maybe_initialize(self):
        if self._loop and self._loop.initialized and not self._backend_obj:
            self._backend_obj = self._loop.add_audio_channel(self.mode)
            # TODO: this is a workaround for the fact that immediately after channel creation,
            # we get a placeholder handle because the back-end is still setting it up.
            # We will push the initial volume on the first update call.
            self._initial_volume_pushed = False
            print("Initialize channel!")
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
            raise Exception("Attempting to get data of an invalid audio channel.")
        return self._backend_obj.get_data()
    
    @Slot(float)
    def set_volume(self, volume):
        if self._backend_obj:
            self._backend_obj.set_volume(volume)
        else:
            self._volume = volume
            self.volumeChanged.emit(volume)
    
    @Slot()
    def update_impl(self, state):
        if state.output_peak != self._output_peak:
            self._output_peak = state.output_peak
            self.outputPeakChanged.emit(self._output_peak)
        if not self._initial_volume_pushed:
            self.set_volume(self._volume)
            self._initial_volume_pushed = True
        else:
            if state.volume != self._volume:
                self._volume = state.volume
                self.volumeChanged.emit(self._volume)