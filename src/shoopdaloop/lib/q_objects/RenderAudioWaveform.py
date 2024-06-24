from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer, QLine
from PySide6.QtQuick import QQuickItem, QQuickPaintedItem
from PySide6.QtGui import QPen, QPainter, QColor
from PySide6.QtQml import QJSValue

from .ShoopPyObject import *

from ..logging import Logger
from ..backend_wrappers import ShoopChannelAudioData
import ctypes
import math

from shoopdaloop.lib.init_dynlibs import init_dynlibs
init_dynlibs()

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
        logger.trace(lambda: f'create pyramid ({len(audio_data)} samples)')
        if not audio_data:
            logger.trace(lambda: 'no audio data')
            self.pyramid = None
        elif not isinstance(audio_data, ShoopChannelAudioData):
            logger.trace(lambda: 'audio data is not a ShoopChannelAudioData')
            self.pyramid = None
        elif not self.backend_create_pyramid:
            logger.trace(lambda: 'no back-end acceleration')
            self.pyramid = None
        else:
            if self.pyramid:
                self.backend_destroy_pyramid(self.pyramid)
            self.pyramid = self.backend_create_pyramid(len(audio_data), audio_data.backend_obj[0].data, 2048)
            logger.trace(lambda: 'done creating pyramid')
    
    def destroy(self):
        if self.pyramid:
            self.backend_destroy_pyramid(self.pyramid)
        self.pyramid = None
    
    def __del__(self):
        self.destroy()

class RenderAudioWaveform(ShoopQQuickPaintedItem):
    def __init__(self, parent=None):
        super(RenderAudioWaveform, self).__init__(parent)
        self._input_data = None
        self._samples_per_bin = 1.0
        self._samples_offset = 0
        self._pyramid = Pyramid()
        self._lines = []

        self.inputDataChanged.connect(self.preprocess)
        self.widthChanged.connect(self.update_lines)
        self.samplesOffsetChanged.connect(self.update)
        self.samplesPerBinChanged.connect(self.update)
    
    inputDataChanged = ShoopSignal('QVariant')
    @ShoopProperty('QVariant', notify=inputDataChanged)
    def input_data(self):
        return self._input_data
    @input_data.setter
    def input_data(self, v):
        self._input_data = v
        self.inputDataChanged.emit(self._input_data)

    samplesPerBinChanged = ShoopSignal(float)
    @ShoopProperty(float, notify=samplesPerBinChanged)
    def samples_per_bin(self):
        return self._samples_per_bin
    @samples_per_bin.setter
    def samples_per_bin(self, v):
        if v != self._samples_per_bin:
            self._samples_per_bin = v
            self.samplesPerBinChanged.emit(v)
    
    samplesOffsetChanged = ShoopSignal(int)
    @ShoopProperty(int, notify=samplesOffsetChanged)
    def samples_offset(self):
        return self._samples_offset
    @samples_offset.setter
    def samples_offset(self, v):
        if v != self._samples_offset:
            self._samples_offset = v
            self.samplesOffsetChanged.emit(v)
    
    @ShoopSlot()
    def preprocess(self, *args):
        logger.trace(lambda: 'preprocess')
        if self._input_data:
            self._pyramid.create(self._input_data.data)
            self.update()
    
    @ShoopSlot()
    def update_lines(self):
        logger.trace(lambda: 'update_lines')
        self.pad_lines_to(math.ceil(self.width()))
    
    def pad_lines_to(self, to):
        while len(self._lines) < to:
            self._lines.append(QLine())

    def paint(self, painter):
        logger.trace(lambda: f'paint (off {self._samples_offset}, scale {self._samples_per_bin})')
        if not self._pyramid.pyramid:
            logger.trace(lambda: 'paint: no pyramid')
            return
        
        subsampling_factor = None
        subsampling_idx = None
        n_levels = self._pyramid.pyramid[0].n_levels
        for ii in range(n_levels):
            i = n_levels - 1 - ii
            factor = self._pyramid.pyramid[0].levels[i].subsampling_factor
            subsampling_factor = factor
            subsampling_idx = i
            if (factor <= self._samples_per_bin):
                break
        
        data = self._pyramid.pyramid[0].levels[subsampling_idx]
        self.pad_lines_to(math.ceil(self.width()))

        logger.trace(f'   - {len(self._lines)} line slots, {data.n_samples} samples')

        for i in range(0, min(math.ceil(self.width()), len(self._lines))):
            sample_idx = (float(i) + float(self._samples_offset) / self._samples_per_bin) * self._samples_per_bin / float(subsampling_factor)
            under_idx = min(max(-1, int(math.floor(sample_idx))), data.n_samples)
            over_idx = min(max(-1, int(math.ceil(sample_idx))), data.n_samples)
            under_sample = (data.data[under_idx] if (under_idx >= 0 and under_idx < data.n_samples) else 0.0)
            over_sample = (data.data[over_idx] if (over_idx >= 0 and over_idx < data.n_samples) else under_sample)
            sample = max(under_sample, over_sample)
            if sample < 0.0:
                self._lines[i].setLine(
                    i, int(0.5*self.height()),
                    i, int(0.5*self.height()) + 1
                )
            else:
                self._lines[i].setLine(
                    i, int((0.5 - 0.5*sample) * self.height()),
                    i, int((0.5 + 0.5*sample) * self.height())
                )

        painter.setPen(QPen(QColor(0.7 * 255.0, 0.0, 0.0)))
        painter.drawLines(self._lines)
        painter.drawLine(0, 0.5*self.height(), self.width(), 0.5*self.height())
        
