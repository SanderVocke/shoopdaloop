from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer, QLine
from PySide6.QtQuick import QQuickItem, QQuickPaintedItem
from PySide6.QtGui import QPen, QPainter
from PySide6.QtQml import QJSValue

from .ShoopPyObject import *

from ..logging import Logger
import ctypes
import math

logger = Logger("Frontend.RenderAudioWaveform")

class Pyramid:
    def __init__(self):
        self.backend_create_pyramid = None
        self.backend_create_pyramid = None
        self.pyramid = None
        try:
            import shoop_accelerate
            self.backend_create_pyramid = shoop_accelerate.create_audio_power_pyramid
            self.backend_destroy_pyramid = shoop_accelerate.destroy_audio_power_pyramid
        except Exception as e:
            logger.warning("Unable to load back-end extensions for rendering audio waveforms. Waveforms will not be visible. Error: {}".format(e))
    
    def create(self, audio_data):
        logger.trace(lambda: 'create pyramid')
        if not self.backend_create_pyramid:
            logger.trace(lambda: 'no back-end acceleration')
            self.pyramid = None
        else:
            if self.pyramid:
                self.backend_destroy_pyramid(self.pyramid)
            array_type = ctypes.c_float * len(audio_data)
            array = array_type()
            for i in range(len(audio_data)):
                array[i] = audio_data[i]
            self.pyramid = self.backend_create_pyramid(len(audio_data), array, 2048)
    
    def destroy(self):
        if self.pyramid:
            self.backend_destroy_pyramid(self.pyramid)
        self.pyramid = None
    
    def __del__(self):
        self.destroy()

class RenderAudioWaveform(ShoopQQuickPaintedItem):
    def __init__(self, parent=None):
        super(RenderAudioWaveform, self).__init__(parent)
        self._input_data = []
        self._samples_per_bin = 1.0
        self._samples_offset = 0
        self._pyramid = Pyramid()
        self._lines = []

        self.inputDataChanged.connect(self.preprocess)
        self.widthChanged.connect(self.update_lines)
        self.samplesOffsetChanged.connect(self.update)
        self.samplesPerBinChanged.connect(self.update)
    
    inputDataChanged = Signal('QVariant')
    @Property('QVariant', notify=inputDataChanged)
    def input_data(self):
        return self._input_data
    @input_data.setter
    def input_data(self, v):
        if isinstance(v, QJSValue):
            v = v.toVariant()
        self._input_data = v
        self.inputDataChanged.emit(self._input_data)

    samplesPerBinChanged = Signal(float)
    @Property(float, notify=samplesPerBinChanged)
    def samples_per_bin(self):
        return self._samples_per_bin
    @samples_per_bin.setter
    def samples_per_bin(self, v):
        pass
    
    samplesOffsetChanged = Signal(int)
    @Property(int, notify=samplesOffsetChanged)
    def samples_offset(self):
        return self._samples_offset
    @samples_offset.setter
    def samples_offset(self, v):
        if v != self._samples_offset:
            self._samples_offset = v
            self.samplesOffsetChanged.emit(v)
    
    @Slot()
    def preprocess(self):
        logger.trace(lambda: 'preprocess')
        self._pyramid.create(self._input_data)
    
    @Slot()
    def update_lines(self):
        logger.trace(lambda: 'update_lines')
        self.pad_lines_to(math.ceil(self.width()))
    
    def pad_lines_to(self, to):
        while len(self._lines) < to:
            self._lines.append(QLine())

    def paint(self, painter):
        logger.trace('paint')
        if not self._pyramid.pyramid:
            logger.trace(lambda: 'paint: no pyramid')
            return
        
        subsampling_factor = None
        for i in range(self._pyramid.pyramid[0].n_levels):
            factor = self._pyramid.pyramid[0].levels[i].subsampling_factor
            if factor <= self._samples_per_bin:
                subsampling_factor = factor
        
        if not subsampling_factor:
            logger.trace(lambda: 'paint: did not find subsampling factor')
            return
        
        data = self._pyramid.pyramid[0].levels[i]
        self.pad_lines_to(math.ceil(self.width()))
        for i in range(0, min(math.ceil(self.width()), len(self._lines))):
            sample_idx = (float(i) + float(self._samples_offset) / self._samples_per_bin) * self._samples_per_bin / float(subsampling_factor)
            nearest_idx = min(max(0, int(round(sample_idx))), data.n_samples)
            sample = (data.data[nearest_idx] if nearest_idx < data.n_samples else 0.0)
            if sample < 0.0:
                self._lines[i].setLine(
                    i, int(0.5*self.height()),
                    i, int(0.5*self.height())
                )
            else:
                self._lines[i].setLine(
                    i, int((0.5 - 0.5*sample) * self.height()),
                    i, int((0.5 + 0.5*sample) * self.height())
                )

        painter.setPen(QPen("red"))
        painter.drawLines(self._lines)
        
