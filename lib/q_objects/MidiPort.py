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
class MidiPort(Port):
    def __init__(self, parent=None):
        super(MidiPort, self).__init__(parent)
        print("HELLO WORLD")
        self._n_events_triggered = 0

    ######################
    # PROPERTIES
    ######################

    # Number of events triggered since last update
    nEventsTriggeredChanged = pyqtSignal(int)
    @pyqtProperty(float, notify=nEventsTriggeredChanged)
    def n_events_triggered(self):
        return self._n_events_triggered
    @n_events_triggered.setter
    def n_events_triggered(self, s):
        if self._n_events_triggered != s:
            self._n_events_triggered = s
            self.nEventsTriggeredChanged.emit(s)
    
    ###########
    ## SLOTS
    ###########

    # Update mode from the back-end.
    @pyqtSlot()
    def update(self):
        state = self._backend_port.get_state()
        self.n_events_triggered = state.n_events_triggered
        self.name = state.name
    
    ##########
    ## INTERNAL MEMBERS
    ##########
    def maybe_initialize_impl(self, name_hint, direction):
        print("OPENING MIDI {}".format(name_hint))
        self._backend_obj = backend.open_midi_port(name_hint, direction)
        print("OPENED")