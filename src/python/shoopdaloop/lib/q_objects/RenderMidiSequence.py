from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer, QRect, Qt
from PySide6.QtGui import QPen, QPainter
from PySide6.QtQml import QJSValue

from .ShoopPyObject import *

from ..logging import Logger
from ..midi_helpers import msgs_to_notes
import copy

logger = Logger("Frontend.RenderMidiSequence")

class RenderMidiSequence(ShoopQQuickPaintedItem):
    def __init__(self, parent=None):
        super(RenderMidiSequence, self).__init__(parent)
        self._messages = []
        self._notes = []
        self._samples_per_bin = 1.0
        self._samples_offset = 0

        self.widthChanged.connect(self.update)
        self.samplesOffsetChanged.connect(self.update)
        self.samplesPerBinChanged.connect(self.update)
        self.notesChanged.connect(self.update)
    
    messagesChanged = ShoopSignal('QVariant')
    @ShoopProperty('QVariant', notify=messagesChanged)
    def messages(self):
        return self._messages
    @messages.setter
    def messages(self, v):
        logger.trace(lambda: "messages changed")
        if isinstance(v, QJSValue):
            v = v.toVariant()
        logger.trace(lambda: "messages changed (have {})".format(len(v)))
        self._messages = (v.data if v else [])
        self.messagesChanged.emit(self._messages)
        self.parse()
        
    notesChanged = ShoopSignal('QVariant')
    @ShoopProperty('QVariant', notify=notesChanged)
    def notes(self):
        return self._notes

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
    def parse(self):
        prev_notes = copy.deepcopy(self._notes)
        logger.debug(lambda: f'Parse messages: {self._messages}')
        self._notes = msgs_to_notes(self._messages)
        if self._notes != prev_notes:
            logger.trace(lambda: "notes changed (have {})".format(len(self._notes)))
            self.notesChanged.emit(self._notes)

    def paint(self, painter):
        logger.trace(f'paint (off {self._samples_offset}, scale {self._samples_per_bin})')
        if not self._notes:
            logger.trace(lambda: 'paint: no notes')
            return

        notes = copy.deepcopy(self._notes)
        
        for note in notes:            
            scaled_start = (note['start'] - self._samples_offset) / self._samples_per_bin
            scaled_end = (note['end'] - self._samples_offset) / self._samples_per_bin
            scaled_start = max(0, scaled_start)
            scaled_end = min(self.width(), scaled_end)
            note['scaled_start'] = scaled_start
            note['scaled_end'] = scaled_end
            
        filtered = [n for n in notes if n['scaled_start'] < self.width() and n['scaled_end'] > 0]
        
        _min = 128
        _max = 0
        for n in notes:
            _min = min(_min, n['note'])
            _max = max(_max, n['note'])

        if _min == _max:
            _min -= 1
            _max += 1
        
        note_thickness = min(15, max(3, int(self.height()*0.8 / (_max - _min))))
        
        rects = []
        for note in filtered:            
            y_rel = (note['note'] - _min) / (_max - _min)
            y_inv = (self.height() * 0.1) + y_rel*(self.height() * 0.8)
            y = self.height() - y_inv
            
            rects.append(QRect(
                note['scaled_start'],
                y - note_thickness/2,
                note['scaled_end'] - note['scaled_start'],
                note_thickness
            ))

        for rect in rects:
            painter.fillRect(rect, Qt.blue)
        
