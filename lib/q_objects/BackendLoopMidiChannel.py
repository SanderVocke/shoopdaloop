from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
from .BackendMidiPort import BackendMidiPort
import re
import time
import os
import tempfile
import json
from typing import *

import sys
sys.path.append('../..')

import lib.backend as backend

# Wraps a back-end loop midi channel.
class BackendLoopMidiChannel(QObject):
    # Other signals
    cycled = pyqtSignal()
    passed_halfway = pyqtSignal()

    def __init__(self, backend_obj : 'backend.BackendLoopMidiChannel', parent=None):
        super(BackendLoopMidiChannel, self).__init__(parent)
        self._n_events_triggered = 0
        self._backend_obj = backend_obj

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

    @pyqtSlot(BackendMidiPort)
    def connect(self, midi_port):
        backend_channel = self._backend_obj
        backend_port = midi_port.get_backend_obj()
        backend_channel.connect(backend_port)
    
    @pyqtSlot()
    def update(self):
        state = self._backend_obj.get_state()

        if state.n_events_triggered != self._n_events_triggered:
            self._n_events_triggered = state.n_events_triggered
            self.nEventsTriggeredChanged.emit(self._n_events_triggered)
    