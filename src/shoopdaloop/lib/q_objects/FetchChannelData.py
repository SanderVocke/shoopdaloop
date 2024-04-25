import queue
from dataclasses import dataclass
from typing import *
import functools
import numpy as np
import math

from PySide6.QtCore import Qt, QObject, QMetaObject, Q_ARG, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem
from PySide6.QtGui import QImage

from .ShoopPyObject import *

from .LoopChannel import LoopChannel
from ..logging import Logger

from ..backend_wrappers import *
from ..mode_helpers import *

# Encapsulate the data in a class. We don't want to convert it to Javascript data,
# rather just pass the Python handle around.
class FetchedData():
    def __init__(self):
        self.data = None

# Wraps a back-end loop channel.
class FetchChannelData(ShoopQQuickItem):
    def __init__(self, parent=None):
        super(FetchChannelData, self).__init__(parent)
        self._channel = None
        self._data = None
        self._dirty = True
        self._active = False
        self._fetching = False
        self._requested_sequence_nr = 1
        self._retrieved_sequence_nr = 0
        self._loop = None
        self._data_as_qimage = None        
        self.logger = Logger('Frontend.FetchChannelData')
        
        self._fetch_timer = QTimer()
        self._fetch_timer.interval = 100
        self._fetch_timer.singleShot = True
        self._fetch_timer.timeout.connect(self.tick)
        self._fetch_timer.start()
        self.tick()
    
    ######################
    # PROPERTIES
    ######################

    # channel
    channelChanged = ShoopSignal('QVariant')
    @ShoopProperty('QVariant', notify=channelChanged)
    def channel(self):
        return self._channel
    @channel.setter
    def channel(self, l):
        if l != self._channel:
            if self._channel and self._channel.isValid():
                self._channel.disconnect(self)
            if self._loop and self._loop.isValid():
                self._loop.disconnect(self)
                self._loop = None
            self._channel = l
            if self._channel and self._channel.isValid():
                self._loop = self._channel.loop
                self._channel.loopChanged.connect(self.set_loop)
                self._channel.dataDirtyChanged.connect(self.set_dirty)
                self._channel.initializedChanged.connect(self.maybe_start_fetch)
                self._channel.loopModeChanged.connect(self.maybe_start_fetch)
                self.dirty = self._channel.data_dirty
                self.channelChanged.emit(l)
                self.channel_data = None
                self.dirty = True
                self.maybe_start_fetch()
    
    # channel_data
    # set automatically. Will hold the most recently
    # fetched data handle.
    channelDataChanged = ShoopSignal('QVariant')
    @ShoopProperty('QVariant', notify=channelDataChanged)
    def channel_data(self):
        return self._data
    @channel_data.setter
    def channel_data(self, l):
        if l != self._data:
            self._data = l
            self.channelDataChanged.emit(l)
    
    # dirty
    # the dirty flag is automatically set based on
    # the incoming dirty status of the back-end.
    # if active == true, when the data turns dirty a
    # new fetch is started.
    dirtyChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=dirtyChanged)
    def dirty(self):
        return self._dirty
    @dirty.setter
    def dirty(self, l):
        if l != self._dirty:
            self._dirty = l
            self.dirtyChanged.emit(l)
            if l:
                self.maybe_start_fetch()
    
    # active
    # if active is false, data will never be fetched.
    activeChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=activeChanged)
    def active(self):
        return self._active
    @active.setter
    def active(self, l):
        if l != self._active:
            self._active = l
            self.activeChanged.emit(l)
            if l:
                self.maybe_start_fetch()
    
    # fetching
    # set automatically. Indicates whether data is
    # currently being fetched.
    fetchingChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=fetchingChanged)
    def fetching(self):
        return self._fetching
    @fetching.setter
    def fetching(self, l):
        if l != self._fetching:
            self._fetching = l
            self.fetchingChanged.emit(l)
    
    # data_as_qimage
    # if the data was a list of floats, this provides
    # a QImage of 8192 elems wide, ARGB32 format,
    # where each pixel holds the float in 32-bit integer range.
    dataAsQImageChanged = ShoopSignal('QVariant')
    @ShoopProperty('QVariant', notify=dataAsQImageChanged)
    def data_as_qimage(self):
        return self._data_as_qimage
    @data_as_qimage.setter
    def data_as_qimage(self, l):
        if l != self._data_as_qimage:
            self._data_as_qimage = l
            self.dataAsQImageChanged.emit(l)
    
    ######################
    # INTERNAL FUNCTIONS
    ######################

    @ShoopSlot(bool)
    def set_dirty(self, dirty):
        self.dirty = dirty
        
    @ShoopSlot('QVariant')
    def set_loop(self, loop):
        self.loop = loop

    @ShoopSlot('QVariant', int)
    def fetch_finished(self, data, seq_nr):
        if seq_nr >= self._requested_sequence_nr:
            self.channel_data = data
            self._retrieved_sequence_nr = seq_nr
        self.fetching = False
        self._fetch_timer.start()        

    @ShoopSlot()
    def maybe_start_fetch(self, *args):
        # Queue a request by incrementing sequence number
        self._requested_sequence_nr += 1
        self.start_timer()
        
    @ShoopSlot()
    def start_timer(self):
        self._fetch_timer.start()
        
    @ShoopSlot()
    def tick(self):
        if not self.channel or not self.channel.isValid() or not self.dirty or not self.active or \
           not self.channel.initialized or not self.channel.loop or not self.channel.loop.isValid() or \
           not self.channel.loop.initialized or is_recording_mode(self.channel.loop_mode) or \
           self._retrieved_sequence_nr >= self._requested_sequence_nr:
            self._fetch_timer.start() # Try again soon
            return
        
        self._fetch_timer.stop()        
        self.fetching = True
        self.channel.clear_data_dirty()

        def worker(channel=self.channel, seq_nr=self._requested_sequence_nr, cb_to=self, logger=self.logger):
            try:
                logger.debug(lambda: "Fetching channel data to front-end.")
                data = FetchedData()
                data.data = channel.get_display_data()
                logger.debug(lambda: "Fetched")
                QMetaObject.invokeMethod(
                    cb_to,
                    'fetch_finished',
                    Qt.QueuedConnection,
                    Q_ARG('QVariant', data),
                    Q_ARG(int, seq_nr)
                )
                logger.debug(lambda: "called back")
            finally:
                QMetaObject.invokeMethod(
                    cb_to,
                    'start_timer',
                    Qt.QueuedConnection
                )
        threading.Thread(target=worker).start()



