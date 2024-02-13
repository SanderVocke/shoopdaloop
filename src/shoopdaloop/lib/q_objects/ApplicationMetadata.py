from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer, QEvent, Qt
from PySide6.QtQml import QJSValue

from .ShoopPyObject import *
class ApplicationMetadata(ShoopQObject):
    def __init__(self, parent=None):
        super(ApplicationMetadata, self).__init__(parent)
        self._version_string = ""
    
    # version_string
    versionStringChanged = ShoopSignal(str)
    @ShoopProperty(str, notify=versionStringChanged)
    def version_string(self):
        return self._version_string
    @version_string.setter
    def version_string(self, l):
        if l != self._version_string:
            self._version_string = l
            self.versionStringChanged.emit(l)
