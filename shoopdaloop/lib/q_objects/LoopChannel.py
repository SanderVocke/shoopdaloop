

import re
import time
import os
import tempfile
import json
from typing import *
import sys

from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from .AudioPort import AudioPort
from .Loop import Loop

from ..backend_wrappers import *
from ..findFirstParent import findFirstParent

# Wraps a back-end loop channel.
class LoopChannel(QQuickItem):
    def __init__(self, parent=None):
        super(LoopChannel, self).__init__(parent)
        self._backend_obj = None
        self._loop = None
        self._mode = ChannelMode.Disabled
        self._connected_ports = []
        self._ports = []
        self._data_length = 0
        self._start_offset = 0

        self.rescan_parents()
        if not self._loop:
            self.parentChanged.connect(self.rescan_parents)
    
    requestBackendInit = Signal() # This signal requests the loop to be instantiated in the backend

    def maybe_initialize(self):
        raise Exception("Unimplemented for base class")

    ######################
    # PROPERTIES
    ######################

    # initialized
    initializedChanged = Signal(bool)
    @Property(bool, notify=initializedChanged)
    def initialized(self):
        return bool(self._backend_obj)

    # loop
    loopChanged = Signal(Loop)
    @Property(Loop, notify=loopChanged)
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
    modeChanged = Signal(int)
    @Property(int, notify=modeChanged)
    def mode(self):
        return self._mode.value
    # indirect setter via back-end
    @Slot(int)
    def set_mode(self, mode):
        _mode = ChannelMode(mode)
        if _mode != self._mode:
            if self._backend_obj:
                self._backend_obj.set_mode(_mode)
            else:
                self.initializedChanged.connect(lambda: self.set_mode(_mode))
    
    # data length
    dataLengthChanged = Signal(int)
    @Property(int, notify=dataLengthChanged)
    def data_length(self):
        return self._data_length
    
    # start offset
    startOffsetChanged = Signal(int)
    @Property(int, notify=startOffsetChanged)
    def start_offset(self):
        return self._start_offset
    # indirect setter via back-end
    @Slot(int)
    def set_start_offset(self, offset):
        if offset != self._start_offset:
            if self._backend_obj:
                self._backend_obj.set_start_offset(offset)
            else:
                self.initializedChanged.connect(lambda: self.set_start_offset(offset))

    # connected ports
    connectedPortsChanged = Signal(list)
    @Property(list, notify=connectedPortsChanged)
    def connected_ports(self):
        return self._connected_ports
    
    # ports to connect
    portsChanged = Signal(list)
    @Property(list, notify=portsChanged)
    def ports(self):
        return self._ports
    @ports.setter
    def ports(self, p):
        for port in self._connected_ports:
            if p and port and not port in p:
                self.disconnect(port)
        for port in p:
            if p and port and not port in self._connected_ports:
                self.connect_port(port)
        self._ports = p
    
    ######################
    # SLOTS
    ######################

    @Slot()
    def initialize(self):
        self.maybe_initialize()

    @Slot('QVariant')
    def connect_port(self, port):
        if not self._backend_obj:
            self.initializedChanged.connect(lambda: self.connect_port(port))
        elif not port.initialized:
            port.initializedChanged.connect(lambda: self.connect_port(port))
        elif port not in self._connected_ports:
            backend_channel = self._backend_obj
            backend_port = port.get_backend_obj()
            backend_channel.connect(backend_port)
            self._connected_ports.append(port)
            self.connectedPortsChanged.emit(self._connected_ports)
    
    @Slot(result='QVariant')
    def get_backend(self):
        maybe_backend = findFirstParent(self, lambda p: p and isinstance(p, QQuickItem) and p.inherits('Backend'))
        if maybe_backend:
            return maybe_backend
        raise Exception("Could not find backend!")
    
    @Slot('QVariant')
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

    @Slot()
    def update_impl(self, state):
        raise Exception("Not implemented in base class")
    
    @Slot()
    def update(self):
        if not self._backend_obj:
            return
        state = self._backend_obj.get_state()

        if state.length != self._data_length:
            self._data_length = state.length
            self.dataLengthChanged.emit(self._data_length)
        if state.start_offset != self._start_offset:
            self._start_offset = state.start_offset
            self.startOffsetChanged.emit(self._start_offset)
        if state.mode != self._mode:
            self._mode = state.mode
            self.modeChanged.emit(self._mode.value)
        
        self.update_impl(state)
    
    @Slot()
    def close(self):
        if self._backend_obj:
            self._backend_obj.destroy()
            self._backend_obj = None
    
    @Slot()
    def rescan_parents(self):
        maybe_loop = findFirstParent(self, lambda p: p and isinstance(p, QQuickItem) and p.inherits('Loop') and self._loop == None)
        if maybe_loop:
            self.loop = maybe_loop