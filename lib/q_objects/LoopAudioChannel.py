from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
from PyQt6.QtQuick import QQuickItem
from .AudioPort import AudioPort
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

# Wraps a back-end loop audio channel.
class LoopAudioChannel(QQuickItem):
    # Other signals
    cycled = pyqtSignal()
    passed_halfway = pyqtSignal()

    def __init__(self, parent=None):
        super(LoopAudioChannel, self).__init__(parent)
        self._output_peak = 0.0
        self._volume = 0.0
        self._backend_obj = None        
        self._loop = None
        self._connected_ports = []
        self._ports = []
    
    def maybe_initialize(self):
        if self._loop and not self._backend_obj:
            self._backend_obj = self._loop.add_audio_channel(True)
            self.initializedChanged.emit(True)

    ######################
    # PROPERTIES
    ######################

    # initialized
    initializedChanged = pyqtSignal(bool)
    @pyqtProperty(bool, notify=initializedChanged)
    def initialized(self):
        return bool(self._backend_obj)

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
            self.maybe_initialize()
    
    # connected ports
    connectedPortsChanged = pyqtSignal(list)
    @pyqtProperty(list, notify=connectedPortsChanged)
    def connected_ports(self):
        return self._connected_ports
    
    # ports to connect
    portsChanged = pyqtSignal(list)
    @pyqtProperty(list, notify=portsChanged)
    def ports(self):
        return self._ports
    @ports.setter
    def ports(self, p):
        for port in self._connected_ports:
            if not port in p:
                self.disconnect(port)
        for port in p:
            if not port in self._connected_ports:
                self.connect(port)
        self._ports = p

    # output peak
    outputPeakChanged = pyqtSignal(float)
    @pyqtProperty(float, notify=outputPeakChanged)
    def output_peak(self):
        return self._output_peak
    
    # volume
    volumeChanged = pyqtSignal(float)
    @pyqtProperty(float, notify=volumeChanged)
    def volume(self):
        return self._volume
    
    ######################
    # SLOTS
    ######################

    @pyqtSlot()
    def initialize(self):
        self.maybe_initialize()

    @pyqtSlot(AudioPort)
    def connect(self, audio_port):
        if not self._backend_obj:
            self.initializedChanged.connect(lambda: self.connect(audio_port))
        elif audio_port not in self._connected_ports:
            backend_channel = self._backend_obj
            backend_port = audio_port.get_backend_obj()
            backend_channel.connect(backend_port)
            self._connected_ports.append(audio_port)
            self.connectedPortsChanged.emit(self._connected_ports)
    
    @pyqtSlot(AudioPort)
    def disconnect(self, audio_port):
        if not self._backend_obj:
            self.initializedChanged.connect(lambda: self.disconnect(audio_port))
        elif audio_port in self._connected_ports:
            backend_channel = self._backend_obj
            backend_port = audio_port.get_backend_obj()
            backend_channel.disconnect(backend_port)
            self._connected_ports.remove(audio_port)
            self.connectedPortsChanged.emit(self._connected_ports)
    
    @pyqtSlot(int, int, int, result=list)
    def get_rms_data(self, from_sample, to_sample, samples_per_bin):
        if not self._backend_obj:
            raise Exception("Attempting to get data of an invalid audio channel.")
        backend_channel = self._backend_obj
        return backend_channel.get_rms_data(from_sample, to_sample, samples_per_bin)
    
    @pyqtSlot(list)
    def load_data(self, data):
        if not self._backend_obj:
            raise Exception("Attempting to load data into an invalid audio channel.")
        backend_channel = self._backend_obj
        backend_channel.load_data(data)
    
    @pyqtSlot()
    def update(self):
        if not self._backend_obj:
            return
        state = self._backend_obj.get_state()

        if state.output_peak != self._output_peak:
            self._output_peak = state.output_peak
            self.outputPeakChanged.emit(self._output_peak)
    
    @pyqtSlot()
    def close(self):
        self._backend_obj.destroy()
        self._backend_obj = None