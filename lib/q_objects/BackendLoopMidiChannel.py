from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
import re
import time
import os
import tempfile
import json

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
    @n_events_triggered.setter
    def n_events_triggered(self, s):
        if self._n_events_triggered != s:
            self._n_events_triggered = s
            self.nEventsTriggeredChanged.emit(s)
    
    #######################
    ## SLOTS
    #######################
    