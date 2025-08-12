
import re
import time
import os
import tempfile
import json
from typing import *
import sys

from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer, SIGNAL, SLOT
from PySide6.QtQuick import QQuickItem

from .LoopChannel import LoopChannel
from .ShoopPyObject import *

from ..midi_helpers import msgs_to_notes
from ..backend_wrappers import *

# Wraps a back-end loop midi channel.
class LoopMidiChannel(LoopChannel):

    def __init__(self, parent=None):
        super(LoopMidiChannel, self).__init__(parent)
        self._n_events_triggered = self._new_n_events_triggered = 0
        self._n_notes_active = self._new_n_notes_active = 0
        self.logger = Logger("Frontend.MidiChannel")
        
    def connect_backend_updates(self):
        QObject.connect(self._backend, SIGNAL("updated_on_gui_thread()"), self, SLOT("updateOnGuiThread()"), Qt.DirectConnection)
        QObject.connect(self._backend, SIGNAL("updated_on_backend_thread()"), self, SLOT("updateOnOtherThread()"), Qt.DirectConnection)
    
    def maybe_initialize(self):
        self._is_midi = True
        if self._backend and self._backend.property('ready') and self._loop and self._loop.property("initialized") and not self._backend_obj:
            from shoop_rust import shoop_rust_add_loop_midi_channel
            from shiboken6 import getCppPointer
            try:
                self._backend_obj = shoop_rust_add_loop_midi_channel(
                    getCppPointer(self._loop)[0],
                    int(self.mode)
                )
                self.connect_backend_updates()
                self.logger.debug(lambda: "Initialized back-end channel")
                self.initializedChanged.emit(True)
            except Exception as e:
                self.logger.debug(lambda: f"Couldn't initialize back-end channel: {e}")

    ######################
    # PROPERTIES
    ######################

    # # of events triggered since last update
    nEventsTriggeredChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=nEventsTriggeredChanged, thread_protection=ThreadProtectionType.AnyThread)
    def n_events_triggered(self):
        return self._n_events_triggered
    
    # # number of notes currently being played
    nNotesActiveChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=nNotesActiveChanged, thread_protection=ThreadProtectionType.AnyThread)
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
        if not self.isValid():
            return
        if self._new_n_events_triggered != self._n_events_triggered:
            self._n_events_triggered = self._new_n_events_triggered
            self.nEventsTriggeredChanged.emit(self._n_events_triggered)
        if self._new_n_notes_active != self._n_notes_active:
            self._n_notes_active = self._new_n_notes_active
            self.nNotesActiveChanged.emit(self._n_notes_active)
    
    @ShoopSlot(result=list, thread_protection=ThreadProtectionType.AnyThread)
    def get_all_midi_data(self):
        if self._backend_obj:
            return midi_msgs_list_from_backend(self._backend_obj.get_all_midi_data())
        else:
            raise Exception("Getting data of un-loaded MIDI channel")

    @ShoopSlot(result=list, thread_protection=ThreadProtectionType.AnyThread)
    def get_recorded_midi_msgs(self):
        all = self.get_all_midi_data()
        return [msg for msg in all if msg['time'] >= 0]
    
    @ShoopSlot(result=list, thread_protection=ThreadProtectionType.AnyThread)
    def get_state_midi_msgs(self):
        all = self.get_all_midi_data()
        return [msg for msg in all if msg['time'] < 0]

    @ShoopSlot(list, thread_protection=ThreadProtectionType.AnyThread)
    def load_all_midi_data(self, data):
        self.requestBackendInit.emit()
        converted = midi_msgs_list_to_backend(data)
        if self._backend_obj:
            self._backend_obj.load_all_midi_data(converted)
        else:
            self.initializedChanged.connect(lambda: self._backend_obj.load_all_midi_data(converted))
    
    @ShoopSlot()
    def reset_state_tracking(self):
        self._backend_obj.reset_state_tracking()
    
    def get_display_data(self):
        return self.get_recorded_midi_msgs()