import queue
from dataclasses import dataclass
from typing import *
import functools
import numpy as np
import math

from PySide6.QtCore import Qt, QObject, QMetaObject, Q_ARG, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem
from PySide6.QtGui import QImage

from .LoopChannel import LoopChannel

from ..backend_wrappers import *
from ..mode_helpers import *

# Wraps a back-end loop channel.
class FetchChannelData(QQuickItem):
    def __init__(self, parent=None):
        super(FetchChannelData, self).__init__(parent)
        self._channel = None
        self._data = None
        self._dirty = True
        self._active = False
        self._fetching = False
        self._request_queue = queue.Queue()
        self._active_request_seq_nr = None
        self._next_seq_nr = 0
        self._loop = None
        self._data_as_qimage = None
    
    ######################
    # PROPERTIES
    ######################

    # channel
    channelChanged = Signal('QVariant')
    @Property('QVariant', notify=channelChanged)
    def channel(self):
        return self._channel
    @channel.setter
    def channel(self, l):
        if l != self._channel:
            if self._channel:
                self._channel.disconnect(self)
            if self._loop:
                self._loop.disconnect(self)
                self._loop = None
            self._channel = l
            if self._channel:
                self._loop = self._channel.loop
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
    # fetched data.
    channelDataChanged = Signal('QVariant')
    @Property('QVariant', notify=channelDataChanged)
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
    dirtyChanged = Signal(bool)
    @Property(bool, notify=dirtyChanged)
    def dirty(self):
        return self._dirty
    @dirty.setter
    def dirty(self, l):
        if l != self._dirty:
            self._dirty = l
            self.dirtyChanged.emit(l)
            self.maybe_start_fetch()
    
    # active
    # if active is false, data will never be fetched.
    activeChanged = Signal(bool)
    @Property(bool, notify=activeChanged)
    def active(self):
        return self._active
    @active.setter
    def active(self, l):
        if l != self._active:
            self._active = l
            self.activeChanged.emit(l)
            self.maybe_start_fetch()
    
    # fetching
    # set automatically. Indicates whether data is
    # currently being fetched.
    fetchingChanged = Signal(bool)
    @Property(bool, notify=fetchingChanged)
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
    dataAsQImageChanged = Signal('QVariant')
    @Property('QVariant', notify=dataAsQImageChanged)
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

    @Slot(bool)
    def set_dirty(self, dirty):
        self.dirty = dirty

    @Slot('QVariant', int)
    def fetch_finished(self, data, seq_nr):
        if seq_nr == self._active_request_seq_nr:
            self.channel_data = data
            self._active_request_seq_nr = None
            self.fetching = False

    @Slot()
    def maybe_start_fetch(self):
        if not self.channel or not self.dirty or not self.active:
            return
        if not self.channel.initialized:
            return
        if not self.channel.loop:
            return
        if is_recording_mode(self.channel.loop_mode):
            return

        # Queue a request and increment sequence number
        self._active_request_seq_nr = self._next_seq_nr
        self._next_seq_nr += 1
        self.fetching = True

        def worker(channel=self.channel, seq_nr=self._active_request_seq_nr, cb_to=self):
            data = channel.get_data()
            # is_floats = len(data) > 0 and functools.reduce(lambda a,b: a and b, [isinstance(e, float) for e in data])
            # qimage = None
            # if is_floats:            
            #     # List of floats. Also provide a QImage version
            #     scale = pow(2,32)/2 - 1
            #     integers = [int((f * scale)) for f in data]
            #     width = 8192
            #     height = math.ceil(len(data) / width)
            #     elems = width*height
            #     while len(integers) < elems:
            #         integers.append(0)
            #     integers = [i + int(pow(2, 32)/2) for i in integers] # from -1..1 to 0..1
            #     np_integers = np.array(integers, dtype=np.uint32, copy=False)
            #     qimage = QImage(np_integers, width, height, QImage.Format.Format_ARGB32)
            channel.clear_data_dirty()
            QMetaObject.invokeMethod(
                cb_to,
                'fetch_finished',
                Qt.QueuedConnection,
                Q_ARG('QVariant', data),
                Q_ARG(int, seq_nr)
            )
        threading.Thread(target=worker).start()



