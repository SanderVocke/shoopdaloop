import queue
from dataclasses import dataclass
from typing import *
import functools
import numpy as np
import math

from PySide6.QtCore import Qt, QObject, QMetaObject, Q_ARG, Signal, Property, Slot, QTimer
from PySide6.QtQuick import QQuickItem, QQuickPaintedItem
from PySide6.QtGui import QImage

from .LoopChannel import LoopChannel

from ..backend_wrappers import *
from ..mode_helpers import *

# Render audio data (list of floats) into a QImage for display.
class RenderAudioWaveform(QQuickPaintedItem):
    def __init__(self, parent=None):
        super(RenderAudioWaveform, self).__init__(parent)
        self._data = None
        self._image = None
    
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
            self.render()
    
    ######################
    # INTERNAL FUNCTIONS
    ######################

    def render(self):
        is_floats = len(self._data) > 0 and functools.reduce(lambda a,b: a and b, [isinstance(e, float) for e in self._data])
        if not is_floats:
            self.image = None
            return

        # Create the image
        img = QImage(len(self._data), int(self.height()), QImage.Format.Format_RGB32)

        # Calculate signal power and go to dB scale
        # TODO: averaging for zoom
        power = [math.sqrt(f*f) for f in self._data]
        lower_thres = pow(10, -45.0)
        power_db = [20.0 * math.log10(max(f, lower_thres)) for f in power]

        # Put in 0-1 range for rendering
        db_0__1 = [max(1.0 - (-f) / 45.0, 0.0) for f in power_db]

        # Fill the image
        for x, f in enumerate(db_0__1):
            for y in range(int(self.height())):
                low_thres = 0.5 - (f / 2.0)
                high_thres = 0.5 + (f / 2.0)
                is_in_waveform = (y >= low_thres and y <= high_thres)
                img.setPixel(x, y, (0xFFFFFFFF if is_in_waveform else 0x00000000))
        

        self._image = img
    
    def paint(self, painter):
        if self._image:
            painter.drawImage(
                0, 0,                    # target xy
                self._image,             # image
                0, 0,                    # source xy
                int(self.width()), int(self.height())  # source wh
                )