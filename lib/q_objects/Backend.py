from PyQt6.QtCore import Qt, QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
from PyQt6.QtQuick import QQuickItem
import re
import time
import os
import tempfile
import json

import sys
sys.path.append('../..')

import lib.backend_wrappers as backend
from lib.q_objects.AudioPort import AudioPort
from lib.q_objects.MidiPort import MidiPort
from lib.findChildItems import findChildItems

# Wraps the back-end.
class Backend(QQuickItem):
    def __init__(self, client_name_hint : str, parent=None):
        super(Backend, self).__init__(parent)
        self._update_interval_ms = 0
        self._timer = None
        self._initialized = False
        self._client_name_hint = ""
    
    update = pyqtSignal()

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
            self._update_interval_ms = u
            if self._timer:
                self._timer.destroy()
                self._timer = None
            self._timer = QTimer(self)
            self._timer.setSingleShot(False)
            self._timer.timeout.connect(self.doUpdate)
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

    @pyqtSlot()
    def doUpdate(self):
        from lib.q_objects.Loop import Loop
        for loop in findChildItems(self, lambda c: isinstance(c, Loop)):
            loop.update()
        self.update.emit()
    
    @pyqtSlot(result=int)
    def get_sample_rate(self):
        return backend.get_sample_rate()
    
    @pyqtSlot(result='QVariant')
    def maybe_backend_test_audio_system(self):
        return backend.maybe_backend_test_audio_system()