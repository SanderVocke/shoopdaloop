from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
from PyQt6.QtQuick import QQuickItem
import re
import time
import os
import tempfile
import json
from typing import *

import sys
sys.path.append('../..')

import lib.backend as backend

# Wraps a back-end port.
class MidiPort(QQuickItem):
    def __init__(self, parent=None):
        super(MidiPort, self).__init__(parent)
        self._n_events_triggered = 0
        self._name_hint = ''
        self._backend_obj = None
        self._direction = ''
        self._name = ''

    ######################
    # PROPERTIES
    ######################

    # name hint
    nameHintChanged = pyqtSignal(str)
    @pyqtProperty(str, notify=nameHintChanged)
    def name_hint(self):
        return self._name_hint
    @name_hint.setter
    def name_hint(self, n):
        if n != self._name_hint:
            if self._name_hint != '':
                raise Exception('Port name hint may only be set once.')
            self._name_hint = n
            self.maybe_initialize()
    
    # direction
    directionChanged = pyqtSignal(str)
    @pyqtProperty(str, notify=directionChanged)
    def direction(self):
        return self._direction
    @direction.setter
    def direction(self, d):
        if d != self._direction:
            if self._direction != '':
                raise Exception('Port direction may only be set once.')
            if d not in ['in', 'out']:
                raise Exception('Port direction may only be "in" or "out"')
            self._direction = d
            self.maybe_initialize()

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
    
     # Get the wrapped back-end object.
    @pyqtSlot(result=backend.BackendMidiPort)
    def get_backend_obj(self):
        return self._backend_obj
    
     ##########
    ## INTERNAL MEMBERS
    ##########
    def maybe_initialize(self):
        if self._name_hint != '' and self._direction != '' and not self._backend_obj:
            direction = None
            if self._direction == 'in':
                direction = backend.PortDirection.Input
            elif self._direction == 'out':
                direction = backend.PortDirection.Output
            if not direction:
                raise Exception("Invalid direction.")
            self._backend_obj = backend.open_audio_port(self._name_hint, direction)