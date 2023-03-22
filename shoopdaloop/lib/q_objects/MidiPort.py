import re
import time
import os
import tempfile
import json
from typing import *
import sys

from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
from PyQt6.QtQuick import QQuickItem

from .Port import Port

# Wraps a back-end port.
class MidiPort(Port):
    def __init__(self, parent=None):
        super(MidiPort, self).__init__(parent)
        self._n_events_triggered = 0
        self._n_notes_active = 0

    ######################
    # PROPERTIES
    ######################

    # Number of events triggered since last update
    nEventsTriggeredChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=nEventsTriggeredChanged)
    def n_events_triggered(self):
        return self._n_events_triggered
    @n_events_triggered.setter
    def n_events_triggered(self, s):
        if self._n_events_triggered != s:
            self._n_events_triggered = s
            self.nEventsTriggeredChanged.emit(s)
    
    # Number of notes currently being played
    nNotesActiveChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=nNotesActiveChanged)
    def n_notes_active(self):
        return self._n_notes_active
    @n_notes_active.setter
    def n_notes_active(self, s):
        if self._n_notes_active != s:
            self._n_notes_active = s
            self.nNotesActiveChanged.emit(s)
    
    ###########
    ## SLOTS
    ###########

    # Update mode from the back-end.
    @pyqtSlot()
    def update(self):
        if not self.initialized:
            return
        state = self._backend_obj.get_state()
        self.n_events_triggered = state.n_events_triggered
        self.n_notes_active = state.n_notes_active
        self.name = state.name
        self.muted = state.muted
        self.passthrough_muted = state.muted
    
    ##########
    ## INTERNAL MEMBERS
    ##########
    def get_backend_obj(self):
        return self._backend_obj

    def maybe_initialize_impl(self, name_hint, direction):
        self._backend_obj = self.backend.get_backend_obj().open_midi_port(name_hint, direction)

    def connect_passthrough_impl(self, other):
        self._backend_obj.connect_passthrough(other.get_backend_obj())