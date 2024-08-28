from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer, QEvent, Qt
from PySide6.QtQml import QJSValue

from .ShoopPyObject import *
class KeyModifiers(ShoopQObject):
    def __init__(self, parent=None):
        super(KeyModifiers, self).__init__(parent)
        self._shift_pressed = False
        self._control_pressed = False
        self._alt_pressed = False
    
    # shift
    shiftPressedChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=shiftPressedChanged)
    def shift_pressed(self):
        return self._shift_pressed
    @shift_pressed.setter
    def shift_pressed(self, l):
        if l != self._shift_pressed:
            self._shift_pressed = l
            self.shiftPressedChanged.emit(l)
    
    # control
    controlPressedChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=controlPressedChanged)
    def control_pressed(self):
        return self._control_pressed
    @control_pressed.setter
    def control_pressed(self, l):
        if l != self._control_pressed:
            self._control_pressed = l
            self.controlPressedChanged.emit(l)
    
    # alt
    altPressedChanged = ShoopSignal(bool)
    @ShoopProperty(bool, notify=altPressedChanged)
    def alt_pressed(self):
        return self._alt_pressed
    @alt_pressed.setter
    def alt_pressed(self, l):
        if l != self._alt_pressed:
            self._alt_pressed = l
            self.altPressedChanged.emit(l)
    
    def process(self, event):
        if event.type() == QEvent.KeyPress and event.key() == Qt.Key.Key_Shift:
            self.shift_pressed = True
        elif event.type() == QEvent.KeyRelease and event.key() == Qt.Key.Key_Shift:
            self.shift_pressed = False
        elif event.type() == QEvent.KeyPress and event.key() == Qt.Key.Key_Control:
            self.control_pressed = True
        elif event.type() == QEvent.KeyRelease and event.key() == Qt.Key.Key_Control:
            self.control_pressed = False
        elif event.type() == QEvent.KeyPress and event.key() == Qt.Key.Key_Alt:
            self.alt_pressed = True
        elif event.type() == QEvent.KeyRelease and event.key() == Qt.Key.Key_Alt:
            self.alt_pressed = False

