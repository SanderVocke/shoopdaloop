
import re
import time
import os
import tempfile
import json
from typing import *
import sys

from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from .MidiPort import MidiPort
from .LoopChannel import LoopChannel
from .ShoopPyObject import *

from ..midi_helpers import msgs_to_notes

# Wraps a back-end loop midi channel.
class LoopMidiChannel(LoopChannel):

    def __init__(self, parent=None):
        super(LoopMidiChannel, self).__init__(parent)
        self._n_events_triggered = self._new_n_events_triggered = 0
        self._n_notes_active = self._new_n_notes_active = 0
    
    def maybe_initialize(self):
        if self._loop and self._loop.initialized and not self._backend_obj:
            self._backend_obj = self._loop.add_midi_channel(self.mode)
            self.initializedChanged.emit(True)

    ######################
    # PROPERTIES
    ######################

    # # of events triggered since last update
    nEventsTriggeredChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=nEventsTriggeredChanged)
    def n_events_triggered(self):
        return self._n_events_triggered
    
    # # number of notes currently being played
    nNotesActiveChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=nNotesActiveChanged)
    def n_notes_active(self):
        return self._n_notes_active
    
    #######################
    ## SLOTS
    #######################

    @ShoopSlot()
    def update_impl(self, state):
        if state.n_events_triggered != self._n_events_triggered:
            self._n_events_triggered = state.n_events_triggered
            self.nEventsTriggeredChanged.emit(self._n_events_triggered)
        if state.n_notes_active != self._n_notes_active:
            self._n_notes_active = state.n_notes_active
            self.nNotesActiveChanged.emit(self._n_notes_active)
    
    def updateOnOtherThreadSubclassImpl(self, state):
        if state.n_events_triggered != self._new_n_events_triggered:
            self._new_n_events_triggered = state.n_events_triggered
        if state.n_notes_active != self._new_n_notes_active:
            self._new_n_notes_active = state.n_notes_active
    
    def updateOnGuiThreadSubclassImpl(self):
        if self._new_n_events_triggered != self._n_events_triggered:
            self._n_events_triggered = self._new_n_events_triggered
            self.nEventsTriggeredChanged.emit(self._n_events_triggered)
        if self._new_n_notes_active != self._n_notes_active:
            self._n_notes_active = self._new_n_notes_active
            self.nNotesActiveChanged.emit(self._n_notes_active)
    
    @ShoopSlot(result=list, thread_protected=False)
    def get_data(self):
        if self._backend_obj:
            return self._backend_obj.get_data()
        else:
            raise Exception("Getting data of un-loaded MIDI channel")

    @ShoopSlot(list, thread_protected=False)
    def load_data(self, data):
        self.requestBackendInit.emit()
        if self._backend_obj:
            self._backend_obj.load_data(data)
        else:
            self.initializedChanged.connect(lambda: self._backend_obj.load_data(data))