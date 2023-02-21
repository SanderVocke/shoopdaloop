from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
from .MidiPort import MidiPort
import re
import time
import os
import tempfile
import json
from typing import *

import sys
sys.path.append('../..')

import lib.backend as backend
import lib.q_objects.Loop as Loop

# Wraps a back-end loop midi channel.
class LoopMidiChannel(QObject):
    # Other signals
    cycled = pyqtSignal()
    passed_halfway = pyqtSignal()

    def __init__(self, parent=None):
        super(LoopMidiChannel, self).__init__(parent)
        self._n_events_triggered = 0
        self._backend_obj = None
        self._loop = None
        if parent and parent.inherits("Loop"):
            self._loop = parent
            self.maybe_initialize()

    ######################
    # PROPERTIES
    ######################

    # loop
    loopChanged = pyqtSignal(Loop.Loop)
    @pyqtProperty(Loop.Loop, notify=loopChanged)
    def loop(self):
        return self._loop
    @loop.setter
    def loop(self, l):
        if l != self._loop:
            if self._loop or self._backend_obj:
                raise Exception('May not change loop of existing audio channel')
            self._loop = l
            self._backend_obj = l.add_midi_channel(True)

    # # of events triggered since last update
    nEventsTriggeredChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=nEventsTriggeredChanged)
    def n_events_triggered(self):
        return self._n_events_triggered
    
    #######################
    ## SLOTS
    #######################

    @pyqtSlot(MidiPort)
    def connect(self, midi_port):
        if not self._backend_obj:
            raise Exception("Attempting to connect an invalid Midi channel.")
        backend_channel = self._backend_obj
        backend_port = midi_port.get_backend_obj()
        backend_channel.connect(backend_port)
    
    @pyqtSlot()
    def update(self):
        if not self._backend_obj:
            raise Exception("Attempting to update an invalid Midi channel.")
        state = self._backend_obj.get_state()

        if state.n_events_triggered != self._n_events_triggered:
            self._n_events_triggered = state.n_events_triggered
            self.nEventsTriggeredChanged.emit(self._n_events_triggered)

    @pyqtSlot()
    def close(self):
        self._backend_obj.destroy()
        self._backend_obj = None