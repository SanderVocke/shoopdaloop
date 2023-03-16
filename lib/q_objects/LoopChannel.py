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

import lib.backend_wrappers as backend
from lib.findFirstParent import findFirstParent
import lib.q_objects.Loop as Loop

# Wraps a back-end loop channel.
class LoopChannel(QQuickItem):
    def __init__(self, parent=None):
        super(LoopChannel, self).__init__(parent)
        self._backend_obj = None
        self._loop = None
        self._mode = backend.ChannelMode.Disabled
        self._connected_ports = []
        self._ports = []
        self._data_length = 0

        self.rescan_parents()
        if not self._loop:
            self.parentChanged.connect(self.rescan_parents)
    
    requestBackendInit = pyqtSignal() # This signal requests the loop to be instantiated in the backend

    def maybe_initialize(self):
        raise Exception("Unimplemented for base class")

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
        if l and l != self._loop:
            if self._loop or self._backend_obj:
                raise Exception('May not change loop of existing channel')
            self._loop = l
            self.maybe_initialize()

    # mode
    modeChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=modeChanged)
    def mode(self):
        return self._mode.value
    # indirect setter via back-end
    @pyqtSlot(int)
    def set_mode(self, mode):
        _mode = backend.ChannelMode(mode)
        if _mode != self._mode:
            if self._backend_obj:
                self._backend_obj.set_mode(_mode)
            else:
                self.initializedChanged.connect(lambda: self.set_mode(_mode))
    
    # data length
    dataLengthChanged = pyqtSignal(int)
    @pyqtProperty(int, notify=dataLengthChanged)
    def data_length(self):
        return self._data_length

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
    
    ######################
    # SLOTS
    ######################

    @pyqtSlot()
    def initialize(self):
        self.maybe_initialize()

    @pyqtSlot('QVariant')
    def connect(self, port):
        if not self._backend_obj:
            self.initializedChanged.connect(lambda: self.connect(port))
        elif not port.initialized:
            port.initializedChanged.connect(lambda: self.connect(port))
        elif port not in self._connected_ports:
            backend_channel = self._backend_obj
            backend_port = port.get_backend_obj()
            backend_channel.connect(backend_port)
            self._connected_ports.append(port)
            self.connectedPortsChanged.emit(self._connected_ports)
    
    @pyqtSlot(result='QVariant')
    def get_backend(self):
        maybe_backend = findFirstParent(self, lambda p: p and isinstance(p, QQuickItem) and p.inherits('Backend'))
        if maybe_backend:
            return maybe_backend
        raise Exception("Could not find backend!")
    
    @pyqtSlot('QVariant')
    def disconnect(self, port):
        if not self._backend_obj:
            self.initializedChanged.connect(lambda: self.disconnect(port))
        elif not port.initialized:
            port.initializedChanged.connect(lambda: self.disconnect(port))
        elif port in self._connected_ports:
            backend_channel = self._backend_obj
            backend_port = port.get_backend_obj()
            backend_channel.disconnect(backend_port)
            self._connected_ports.remove(port)
            self.connectedPortsChanged.emit(self._connected_ports)

    @pyqtSlot()
    def update_impl(self, state):
        raise Exception("Not implemented in base class")
    
    @pyqtSlot()
    def update(self):
        if not self._backend_obj:
            return
        state = self._backend_obj.get_state()

        if state.length != self._data_length:
            print("length {} -> {}".format(self._data_length, state.length))
            self._data_length = state.length
            self.dataLengthChanged.emit(self._data_length)
        if state.mode != self._mode:
            self._mode = state.mode
            self.modeChanged.emit(self._mode.value)
        
        self.update_impl(state)
    
    @pyqtSlot()
    def close(self):
        if self._backend_obj:
            self._backend_obj.destroy()
            self._backend_obj = None
    
    @pyqtSlot()
    def rescan_parents(self):
        maybe_loop = findFirstParent(self, lambda p: p and isinstance(p, QQuickItem) and p.inherits('Loop') and self._loop == None)
        if maybe_loop:
            self.loop = maybe_loop