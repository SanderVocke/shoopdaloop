

import re
import time
import os
import tempfile
import json
from typing import *
import sys

from PySide6.QtCore import Qt, QObject, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem

from .ShoopPyObject import *

from .AudioPort import AudioPort
from .Loop import Loop

from ..backend_wrappers import *
from ..findFirstParent import findFirstParent
from ..logging import Logger

import traceback

# Wraps a back-end loop channel.
class LoopChannel(ShoopQQuickItem):
    def __init__(self, parent=None):
        super(LoopChannel, self).__init__(parent)
        self._backend_obj = None
        self._loop = None
        self._mode = self._mode = ChannelMode.Disabled
        self._connected_ports = []
        self._ports = []
        self._data_length = self._new_data_length = 0
        self._start_offset = self._new_start_offset = 0
        self._recording_started_at = None
        self._data_dirty = self._new_data_dirty = True
        self._n_preplay_samples = self._new_n_preplay_samples = 0
        self._played_back_sample = self._new_played_back_sample = None
        self._n_pending_updates = 0
        self.__logger = Logger('Frontend.LoopChannel')

        self._signal_sender = ThreadUnsafeSignalEmitter()
        self._signal_sender.signal.connect(self.updateOnGuiThread, Qt.QueuedConnection)
    
    requestBackendInit = ShoopSignal() # This signal requests the loop to be instantiated in the backend
    update = ShoopSignal()

    def maybe_initialize(self):
        self.__logger.throw_error("Unimplemented for base class")

    ######################
    # PROPERTIES
    ######################

    # initialized
    initializedChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=initializedChanged)
    def initialized(self):
        return bool(self._backend_obj)

    # loop
    loopChanged = ShoopSignal('QVariant')
    @ShoopProperty('QVariant', notify=loopChanged)
    def loop(self):
        return self._loop
    @loop.setter
    def loop(self, l):
        if l and l != self._loop:
            if self._loop or self._backend_obj:
                raise Exception('May not change loop of existing channel')
            self._loop = l
            self.loopChanged.emit(self._loop)
            self._loop.modeChanged.connect(self.loopModeChanged)
            self.loopModeChanged.emit(self._loop.mode)
            self.maybe_initialize()
    
    # loop_mode
    loopModeChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=loopModeChanged)
    def loop_mode(self):
        return (self._loop.mode if self._loop else 0)

    # recording_started_at
    recordingStartedAtChanged = ShoopSignal('QVariant')
    @ShoopProperty('QVariant', notify=recordingStartedAtChanged)
    def recording_started_at(self):
        return self._recording_started_at
    @recording_started_at.setter
    def recording_started_at(self, l):
        if l and l != self._recording_started_at:
            self._recording_started_at = l
            self.recordingStartedAtChanged.emit(l)

    # mode
    modeChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=modeChanged)
    def mode(self):
        return self._mode.value
    # indirect setter via back-end
    @ShoopSlot(int)
    def set_mode(self, mode):
        _mode = ChannelMode(mode)
        if _mode != self._mode:
            if self._backend_obj:
                self.__logger.debug(lambda: 'Set mode -> {}'.format(_mode))
                self._backend_obj.set_mode(_mode)
            else:
                self.initializedChanged.connect(lambda: self.set_mode(_mode))
    
    # data length
    dataLengthChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=dataLengthChanged, thread_protection=ThreadProtectionType.AnyThread)
    def data_length(self):
        return self._data_length
    
    # last played sample
    playedBackSampleChanged = ShoopSignal('QVariant')
    @ShoopProperty('QVariant', notify=playedBackSampleChanged)
    def played_back_sample(self):
        return self._played_back_sample
    
    # start offset
    startOffsetChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=startOffsetChanged)
    def start_offset(self):
        return self._start_offset
    # indirect setter via back-end
    @ShoopSlot(int, thread_protection=ThreadProtectionType.AnyThread)
    def set_start_offset(self, offset):
        if offset != self._start_offset:
            if self._backend_obj:
                self.__logger.debug(lambda: 'Set start offset -> {}'.format(offset))
                self._backend_obj.set_start_offset(offset)
            else:
                self.initializedChanged.connect(lambda: self.set_start_offset(offset))
    
    # n preplay samples
    nPreplaySamplesChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=nPreplaySamplesChanged)
    def n_preplay_samples(self):
        return self._n_preplay_samples
    # indirect setter via back-end
    @ShoopSlot(int, thread_protection=ThreadProtectionType.AnyThread)
    def set_n_preplay_samples(self, n):
        if n != self._n_preplay_samples:
            if self._backend_obj:
                self.__logger.debug(lambda: 'Set # preplay samples -> {}'.format(n))
                self._backend_obj.set_n_preplay_samples(n)
            else:
                self.initializedChanged.connect(lambda: self.set_n_preplay_samples(n))

    # data dirty
    dataDirtyChanged = ShoopSignal(bool)
    @ShoopProperty(int, notify=dataDirtyChanged)
    def data_dirty(self):
        return self._data_dirty
    # indirect clear via back-end
    @ShoopSlot(thread_protection=ThreadProtectionType.AnyThread)
    def clear_data_dirty(self):
        if self._backend_obj:
            self.__logger.debug(lambda: 'Set data dirty -> False')
            self._backend_obj.clear_data_dirty()
        else:
            self.initializedChanged.connect(lambda: self.clear_data_dirty())

    # connected ports
    connectedPortsChanged = ShoopSignal(list)
    @ShoopProperty(list, notify=connectedPortsChanged)
    def connected_ports(self):
        return [p for p in self._connected_ports if p and p.isValid()]
    
    # ports to connect
    portsChanged = ShoopSignal(list)
    @ShoopProperty(list, notify=portsChanged)
    def ports(self):
        return [p for p in self._ports if p and p.isValid()]
    @ports.setter
    def ports(self, p):
        self.__logger.debug(lambda: 'ports -> {}'.format(p))
        for port in self._connected_ports:
            if p and port and port.isValid() and not port in p:
                self.disconnect(port)
        for port in p:
            if port and port.isValid() and not port in self._connected_ports:
                self.connect_port(port)
        self._ports = p
    
    ######################
    # SLOTS
    ######################

    @ShoopSlot()
    def clear(self):
        if self._backend_obj:
            self.__logger.debug(lambda: 'Clear')
            self._backend_obj.clear()
        else:
            self.initializedChanged.connect(lambda: self.clear())

    @ShoopSlot()
    def initialize(self):
        self.maybe_initialize()

    @ShoopSlot('QVariant')
    def connect_port(self, port):
        self.__logger.debug(lambda: f'Connect port {port}')
        if not port.isValid():
            return
        if not self._backend_obj:
            self.__logger.debug(lambda: 'Defer connect to port')
            self.initializedChanged.connect(lambda: self.connect_port(port))
        elif not port.initialized:
            self.__logger.debug(lambda: 'Defer connect to port')
            port.initializedChanged.connect(lambda: self.connect_port(port))
        elif port not in self._connected_ports:
            self.__logger.debug(lambda: 'Connect to port')
            backend_channel = self._backend_obj
            backend_port = port.get_backend_obj()
            if not (port.input_connectability & PortConnectability.Internal.value):
                backend_channel.connect_input(backend_port)
            else:
                backend_channel.connect_output(backend_port)
            self._connected_ports.append(port)
            self.connectedPortsChanged.emit(self._connected_ports)
    
    @ShoopSlot(result='QVariant')
    def get_backend(self):
        maybe_backend = findFirstParent(self, lambda p: p and isinstance(p, QQuickItem) and p.inherits('Backend'))
        if maybe_backend:
            return maybe_backend
        self.__logger.throw_error("Could not find backend")
    
    @ShoopSlot('QVariant')
    def disconnect(self, port):
        if not port.isValid():
            return
        if not self._backend_obj:
            self.__logger.debug(lambda: 'Defer disconnect from port')
            self.initializedChanged.connect(lambda: self.disconnect(port))
        elif not port.initialized:
            self.__logger.debug(lambda: 'Defer disconnect from port')
            port.initializedChanged.connect(lambda: self.disconnect(port))
        elif port in self._connected_ports:
            self.__logger.debug(lambda: 'Disconnect from port')
            backend_channel = self._backend_obj
            backend_port = port.get_backend_obj()
            backend_channel.disconnect(backend_port)
            self._connected_ports.remove(port)
            self.connectedPortsChanged.emit(self._connected_ports)

    def updateOnOtherThreadSubclassImpl(self, state):
        self.__logger.throw_error("Not implemented in base class")
    
    def updateOnGuiThreadSubclassImpl(self):
        self.__logger.throw_error("Not implemented in base class")
    
    @ShoopSlot(thread_protection = ThreadProtectionType.OtherThread)
    def updateOnOtherThread(self):
        if not self._backend_obj:
            return
        state = self._backend_obj.get_state()

        self._new_data_length = state.length
        self._new_start_offset = state.start_offset
        self._new_mode = state.mode
        self._new_data_dirty = state.data_dirty
        self._new_n_preplay_samples = state.n_preplay_samples
        self._new_played_back_sample = state.played_back_sample
        self._n_pending_updates += 1
        
        self.updateOnOtherThreadSubclassImpl(state)
        
        self._signal_sender.do_emit()
    
    @ShoopSlot()
    def updateOnGuiThread(self):
        if not self._backend_obj or not self.isValid():
            return
        if self._n_pending_updates == 0:
            return

        if self._new_data_length != self._data_length:
            self._data_length = self._new_data_length
            self.dataLengthChanged.emit(self._data_length)
        if self._new_start_offset != self._start_offset:
            self.__logger.debug(lambda: 'start_offset -> {}'.format(self._new_start_offset))
            self._start_offset = self._new_start_offset
            self.startOffsetChanged.emit(self._start_offset)
        if self._new_mode != self._mode:
            self.__logger.debug(lambda: 'mode -> {}'.format(ChannelMode(self._new_mode)))
            self._mode = self._new_mode
            self.modeChanged.emit(self._mode.value)
        if self._new_data_dirty != self._data_dirty:
            self.__logger.debug(lambda: 'data dirty -> {}'.format(self._new_data_dirty))
            self._data_dirty = self._new_data_dirty
            self.dataDirtyChanged.emit(self._data_dirty)
        if self._new_n_preplay_samples != self._n_preplay_samples:
            self.__logger.debug(lambda: '# preplay samples -> {}'.format(self._new_n_preplay_samples))
            self._n_preplay_samples = self._new_n_preplay_samples
            self.nPreplaySamplesChanged.emit(self._n_preplay_samples)
        if self._new_played_back_sample != self._played_back_sample:
            self._played_back_sample = self._new_played_back_sample
            self.playedBackSampleChanged.emit(self._played_back_sample)
        
        self.updateOnGuiThreadSubclassImpl()
        self._n_pending_updates = 0
    
    @ShoopSlot()
    def close(self):
        if self._backend_obj:
            self.__logger.debug(lambda: 'destroy')
            self._backend_obj.destroy()
            self._backend_obj = None
    
    @ShoopSlot(result='QVariant')
    def get_display_data(self):
        raise NotImplementedError()
