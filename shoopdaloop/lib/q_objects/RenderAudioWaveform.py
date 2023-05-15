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
            self.update()
    

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
            self.update()
    
    ######################
    # INTERNAL FUNCTIONS
    ######################

    def generate_lines(self, samples_per_bin):
        def avg(l):
            return sum(l)/len(l)
        
        n = int(len(self._precalc) / samples_per_bin)
        samples = [avg(self._precalc[x*samples_per_bin:x*samples_per_bin+samples_per_bin]) for x in range(n)]

        return [QLine(x, (0.5 - f/2.0)*self.height(), x, (0.5 + f/2.0)*self.height()) for (x, f) in enumerate(samples)]


    def update_lines(self):
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

        # Generate rendering lines to use with drawLines at different zoom levels
        self._lines = {}
        for level in [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048]:
            self._lines[level] = self.generate_lines(level)
    
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
            print("frame time: {}".format(time.time() - start))