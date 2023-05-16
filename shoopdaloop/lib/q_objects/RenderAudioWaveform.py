import queue
from dataclasses import dataclass
from typing import *
import functools
import numpy as np
import math
import time

from PySide6.QtCore import Qt, QObject, QMetaObject, Q_ARG, Signal, Property, Slot, QTimer, QLine
from PySide6.QtQuick import QQuickItem, QQuickPaintedItem
from PySide6.QtGui import QImage, QPainter, QPen, QBrush, QColor, QColorConstants

from .LoopChannel import LoopChannel

from ..backend_wrappers import *
from ..mode_helpers import *

# Render audio data (list of floats) into a QImage for display.
class RenderAudioWaveform(QQuickPaintedItem):
    def __init__(self, parent=None):
        super(RenderAudioWaveform, self).__init__(parent)
        self._samples_per_bin = 1
        self._data = None
        self._precalc = None
        self._lines = None
        self._offset_samples = 0
    
    ######################
    # PROPERTIES
    ######################
    
    # input_data
    # input data to render.
    inputDataChanged = Signal(list)
    @Property(list, notify=inputDataChanged)
    def input_data(self):
        return self._data
    @input_data.setter
    def input_data(self, l):
        if l != self._data:
            self._data = l
            self.inputDataChanged.emit(l)
            self.update_lines()
            self.update() # QQuickPaintedItem update triggers a render
    

    # samples_per_bin
    samplesPerBinChanged = Signal(int)
    @Property(int, notify=samplesPerBinChanged)
    def samples_per_bin(self):
        return self._samples_per_bin
    @samples_per_bin.setter
    def samples_per_bin(self, l):
        if l != self._samples_per_bin:
            self._samples_per_bin = l
            self.samplesPerBinChanged.emit(l)
            self.update() # QQuickPaintedItem update triggers a render
    
    # offset_samples
    offsetSamplesChanged = Signal(int)
    @Property(int, notify=offsetSamplesChanged)
    def offset_samples(self):
        return self._offset_samples
    @offset_samples.setter
    def offset_samples(self, l):
        if l != self._offset_samples:
            self._offset_samples = l
            self.offsetSamplesChanged.emit(l)
            self.update() # QQuickPaintedItem update triggers a render
    
    ######################
    # INTERNAL FUNCTIONS
    ######################

    def update_lines(self):
        start = time.time()

        is_floats = len(self._data) > 0 and functools.reduce(lambda a,b: a and b, [isinstance(e, float) for e in self._data])
        if not is_floats:
            self.image = None
            return

        # Calculate signal power and go to dB scale
        power = [abs(f) for f in self._data]
        lower_thres = pow(10, -45.0)
        power_db = [20.0 * math.log10(max(f, lower_thres)) for f in power]

        # Put in 0-1 range for rendering
        self._precalc = [max(1.0 - (-f) / 45.0, 0.0) for f in power_db]

        # Generate rendering lines to use with drawLines at different zoom levels.
        self._lines = {}

        def generate_lines(samples):
            print('generating... {}'.format(len(samples)))
            lns = [QLine()] * len(samples)
            for idx in range(len(lns)):
                lns[idx].setLine(idx, (0.5 - samples[idx]/2.0)*self.height(), idx, (0.5 + samples[idx]/2.0)*self.height())
            return lns
            # return [QLine(x, (0.5 - f/2.0)*self.height(), x, (0.5 + f/2.0)*self.height()) for (x, f) in enumerate(samples)]
        
        def reduce_by_2(samples):
            print('reducing... {}'.format(len(samples)))
            if len(samples) < 2:
                return []
            return np.mean(samples.reshape(-1, 2), 1) # averaging reduction by 2
            #return [(samples[x*2] + samples[x*2+1])/2.0 for x in range(int(len(samples)/2))]
        
        factor = 1
        reduced = np.array(self._precalc)
        # Ensure array size is a power of 2
        reduced.resize(pow(2, int(math.ceil(math.log2(len(reduced))))))
        while factor <= 2048:
            self._lines[factor] = generate_lines(reduced)
            factor *= 2
            reduced = reduce_by_2(reduced)
        
        # For performance measuring
        print("RenderAudioWaveform: pre-render time {}".format(time.time() - start))
    
    def paint(self, painter):
        if self._lines:
            start = time.time()

            # background
            painter.fillRect(0, 0, int(self.width()), int(self.height()), Qt.black)

            painter.setPen(QPen(Qt.red))

            # Waveform
            painter.drawLines(
                self._lines[self._samples_per_bin][:int(self.width())]
            )

            # Horizonal line
            painter.drawLine(0, int(self.height()/2), int(self.width()), int(self.height()/2))

            # For performance measuring
            # print("RenderAudioWaveform: frame time {}".format(time.time() - start))