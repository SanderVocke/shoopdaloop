from PySide6.QtCore import QObject, Signal, Property, Slot
from PySide6.QtQml import QJSValue
from .ShoopPyObject import *

from .SettingsIOImpl import *

class SettingsIO(ShoopQObject):
    def __init__(self, parent=None):
        super(SettingsIO, self).__init__(parent)
        self.impl = SettingsIOImpl()
        
    @ShoopSlot('QVariant', 'QVariant')
    def save_settings(self, settings, override_filename=None):
        if isinstance(settings, QJSValue):
            settings = settings.toVariant()
        self.impl.save_settings(settings, override_filename)
    
    @ShoopSlot('QVariant', result='QVariant')
    def load_settings(self, override_filename=None):
        return self.impl.load_settings(override_filename)