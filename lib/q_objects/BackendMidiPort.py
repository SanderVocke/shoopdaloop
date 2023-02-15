from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
import re
import time
import os
import tempfile
import json

import sys
sys.path.append('../..')

import lib.backend as backend

# Wraps a back-end port.
class BackendMidiPort(QObject):
    def __init__(self, backend_port : Type[backend.BackendPort], parent=None):
        super(BackendMidiPort, self).__init__(parent)
        self._n_events_triggered = 0
        self._name = ''
        self._backend_port = backend_port

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
    
    # name
    nameChanged = pyqtSignal(str)
    @pyqtProperty(str, notify=nameChanged)
    def name(self):
        return self._name
    @name.setter
    def volume(self, s):
        if self._name != s:
            self._name = s
            self.nameChanged.emit(s)
    
    ###########
    ## SLOTS
    ###########

    # Update mode from the back-end.
    @pyqtSlot()
    def update(self):
        state = self._backend_port.get_state()
        self.n_events_triggered = state.n_events_triggered
        self.name = state.name