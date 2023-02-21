from PyQt6.QtCore import Qt, QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
import re
import time
import os
import tempfile
import json

import sys
sys.path.append('../..')

import lib.backend as backend
from lib.q_objects.Loop import Loop
from lib.q_objects.AudioPort import AudioPort
from lib.q_objects.MidiPort import MidiPort

# Wraps the back-end.
class Backend(QObject):
    def __init__(self, client_name_hint : str, parent=None):
        super(Backend, self).__init__(parent)
        self._update_interval_ms = 100
        self._timer = None
        self._initialized = False
        self._client_name_hint = ""

    ######################
    # PROPERTIES
    ######################

    updateIntervalMsChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=updateIntervalMsChanged)
    def update_interval_ms(self):
        return self._update_interval_ms
    @update_interval_ms.setter
    def update_interval_ms(self, u):
        if u != self._update_interval_ms:
            if self._timer:
                self._timer.destroy()
                self._timer = None
            self._timer = QTimer(self)
            self._timer.setSingleShot(False)
            self._timer.timeout.connect(self.update)
            self._timer.start(self._update_interval_ms)
    
    initializedChanged = pyqtSignal(bool)
    @pyqtProperty(bool, notify=initializedChanged)
    def initialized(self):
        return self._initialized
    
    clientNameHintChanged = pyqtSignal(str)
    @pyqtProperty(str, notify=clientNameHintChanged)
    def client_name_hint(self):
        return self._client_name_hint
    @client_name_hint.setter
    def client_name_hint(self, n):
        if self._initialized:
            raise Exception("Back-end client name hint may only be set once.")
        self._client_name_hint = n
        backend.init_backend(n)
        self._initialized = True        
    
    ###########
    ## SLOTS
    ###########

    # Request state updates for all back-end objects.
    @pyqtSlot()
    def update(self):
        for loop in self.findChildren(Loop):
            loop.update()
    
    @pyqtSlot(result=int)
    def get_sample_rate(self):
        return backend.get_sample_rate()