from PyQt6.QtCore import Qt, QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
import re
import time
import os
import tempfile
import json

import sys
sys.path.append('../..')

import lib.backend as backend
from lib.q_objects.BackendLoop import BackendLoop
from lib.q_objects.BackendAudioPort import BackendAudioPort
from lib.q_objects.BackendMidiPort import BackendMidiPort

# Wraps a back-end loop.
class Backend(QObject):
    def __init__(self, client_name_hint : str, parent=None):
        super(Backend, self).__init__(parent)
        self._backend_obj = backend.Backend(client_name_hint)

    ######################
    # PROPERTIES
    ######################
    
    ###########
    ## SLOTS
    ###########

    @pyqtSlot(result=BackendLoop)
    def create_loop(self):
        return BackendLoop(self._backend_obj.create_loop(), self)
    
    @pyqtSlot(result=BackendAudioPort)
    def open_audio_port(self, name_hint, direction : backend.PortDirection):
        return BackendAudioPort(self._backend_obj.open_audio_port(name_hint, direction), self)
    
    @pyqtSlot(result=BackendMidiPort)
    def open_midi_port(self, name_hint, direction : backend.PortDirection):
        return BackendMidiPort(self._backend_obj.open_midi_port(name_hint, direction), self)
    
    # Request state updates for all back-end objects.
    @pyqtSlot()
    def update(self):
        loops = self.findChildren(BackendLoop)
        for loop in loops:
            loop.update()
    
    # Start a timer to regularly call update.
    @pyqtSlot(int)
    def start_update_timer(self, interval_ms : int):
        timer = QTimer(self)
        timer.setSingleShot(False)
        timer.timeout.connect(self.update)
        timer.start(interval_ms)