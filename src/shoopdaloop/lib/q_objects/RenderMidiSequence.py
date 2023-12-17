from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer, QLine
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
    
    messagesChanged = Signal('QVariant')
    @Property('QVariant', notify=messagesChanged)
    def messages(self):
        return self._messages
    @messages.setter
    def messages(self, v):
        logger.trace(lambda: "messages changed")
        if isinstance(v, QJSValue):
            v = v.toVariant()
        logger.trace(lambda: "messages changed (have {})".format(len(v)))
        self._messages = (v if v else [])
        self.messagesChanged.emit(self._messages)
        self.parse()
        
    notesChanged = Signal('QVariant')
    @Property('QVariant', notify=notesChanged)
    def notes(self):
        return self._notes

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
    def parse(self):
        prev_notes = copy.deepcopy(self._notes)
        self._notes = msgs_to_notes(self._messages)
        if self._notes != prev_notes:
            logger.trace(lambda: "notes changed (have {})".format(len(self._notes)))
            self.notesChanged.emit(self._notes)

    def paint(self, painter):
        logger.trace('paint')
        if not self._notes:
            logger.trace(lambda: 'paint: no notes')
            return

        lines = []
        for note in self._notes:            
            scaled_start = (note['start'] - self._samples_offset) / self._samples_per_bin
            scaled_end = (note['end'] - self._samples_offset) / self._samples_per_bin
            
            if scaled_end < 0:
                continue
            if scaled_start > self.width():
                continue
            
            scaled_start = max(0, scaled_start)
            scaled_end = min(self.width(), scaled_end)
            
            lines.append(QLine(
                scaled_start,
                0,
                scaled_end,
                0
            ))

        painter.setPen(QPen("blue"))
        painter.drawLines(lines)
        
