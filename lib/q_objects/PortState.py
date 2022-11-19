from PyQt6.QtCore import QObject, pyqtSignal, pyqtProperty, pyqtSlot, QTimer
import re
import time
import os
import tempfile

# Represents the state of a port in the back-end
class PortState(QObject):

    # State change notifications
    volumeChanged = pyqtSignal(float)
    mutedChanged = pyqtSignal(bool)
    passthroughChanged = pyqtSignal(float)
    passthroughMutedChanged = pyqtSignal(bool)

    def __init__(self, parent=None):
        super(PortState, self).__init__(parent)
        self._volume = 1.0
        self._muted = False
        self._passthrough = 1.0
        self._passthroughMuted = False
    
    ######################
    # PROPERTIES
    ######################

    # volume
    @pyqtProperty(float, notify=volumeChanged)
    def volume(self):
        return self._volume
    @volume.setter
    def volume(self, l):
        if self._volume != l:
            self._volume = l
            self.volumeChanged.emit(l)
    
    # passthrough
    @pyqtProperty(float, notify=passthroughChanged)
    def passthrough(self):
        return self._passthrough
    @passthrough.setter
    def passthrough(self, l):
        if self._passthrough != l:
            self._passthrough = l
            self.passthroughChanged.emit(l)
    
    # muted
    @pyqtProperty(bool, notify=mutedChanged)
    def muted(self):
        return self._muted
    @muted.setter
    def muted(self, l):
        if self._muted != l:
            self._muted = l
            self.mutedChanged.emit(l)
    
    # passthroughMuted
    @pyqtProperty(bool, notify=passthroughMutedChanged)
    def passthroughMuted(self):
        return self._passthroughMuted
    @passthroughMuted.setter
    def passthroughMuted(self, l):
        if self._passthroughMuted != l:
            self._passthroughMuted = l
            self.passthroughMutedChanged.emit(l)