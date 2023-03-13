from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
from PyQt6.QtQuick import QQuickItem
from .MidiPort import MidiPort
from .LoopChannel import LoopChannel
import re
import time
import os
import tempfile
import json
from typing import *

import sys
sys.path.append('../..')

import lib.backend_wrappers as backend
import lib.q_objects.Loop as Loop

# Wraps a back-end loop midi channel.
class LoopMidiChannel(LoopChannel):

    def __init__(self, parent=None):
        super(LoopMidiChannel, self).__init__(parent)
        self._n_events_triggered = 0
    
    def maybe_initialize(self):
        if self._loop and self._loop.initialized and not self._backend_obj:
            self._backend_obj = self._loop.add_midi_channel(self.mode)
            self.initializedChanged.emit(True)

    ######################
    # PROPERTIES
    ######################

    # # of events triggered since last update
    nEventsTriggeredChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=nEventsTriggeredChanged)
    def n_events_triggered(self):
        return self._n_events_triggered
    
    #######################
    ## SLOTS
    #######################

    @pyqtSlot()
    def update_impl(self, state):
        if state.n_events_triggered != self._n_events_triggered:
            self._n_events_triggered = state.n_events_triggered
            self.nEventsTriggeredChanged.emit(self._n_events_triggered)
    
    @pyqtSlot(result=list)
    def get_data(self):
        if self._backend_obj:
            return self._backend_obj.get_data()
        else:
            raise Exception("Getting data of un-loaded MIDI channel")